#include <fcb/header.hpp>

#include "detail/checked.hpp"
#include "detail/header_access.hpp"

#include <fcb/generated/header_generated.h>

#include <algorithm>
#include <cstring>
#include <string>

namespace fcb {

/// Leading padding so the size-prefixed FlatBuffer body ends up 8-aligned.
/// See the alignment note in read_header().
static constexpr std::size_t kBodyAlignPad = 4;

const ::Header* HeaderView::raw() const {
    return GetSizePrefixedHeader(buffer_->data() + kBodyAlignPad);
}

const ::Header* detail::HeaderAccess::get(const HeaderView& h) { return h.raw(); }

namespace {

/// Read a double out of a FlatBuffers struct without dereferencing a
/// possibly-misaligned pointer.
///
/// The Rust writer places `Transform` at body+68 -- an odd multiple of 4 --
/// even though it is six doubles and requires 8-byte alignment. That is an
/// INTERNAL offset, so no choice of buffer placement can fix it (confirmed:
/// the C++ verifier's alignment check fails at every start residue mod 8).
/// Calling t->scale().x() therefore binds a reference to a misaligned
/// address, which UBSan flags and which is undefined behaviour even where
/// it happens to work. memcpy is well-defined at any alignment and compiles
/// to the same load on x86/ARM.
double read_f64_at(const void* base, std::size_t byte_offset) {
    double d;
    std::memcpy(&d, static_cast<const std::uint8_t*>(base) + byte_offset, sizeof(double));
    return d;  // FlatBuffers scalars are little-endian; so is every target we support.
}

/// Explicit little-endian assembly rather than memcpy of a uint32_t, so the
/// decode is endian-correct by construction rather than by host accident.
std::uint32_t read_u32_le(const std::vector<std::uint8_t>& b, std::size_t at) {
    return static_cast<std::uint32_t>(b[at]) |
           (static_cast<std::uint32_t>(b[at + 1]) << 8) |
           (static_cast<std::uint32_t>(b[at + 2]) << 16) |
           (static_cast<std::uint32_t>(b[at + 3]) << 24);
}

void fill_columns(const ::Header* hdr, FileInfo& info) {
    const auto* cols = hdr->columns();
    if (cols == nullptr) return;
    info.columns.reserve(cols->size());
    for (const auto* c : *cols) {
        if (c == nullptr) continue;
        ColumnInfo ci{};
        ci.index = c->index();
        ci.name = c->name() != nullptr ? c->name()->str() : std::string();
        ci.type = static_cast<std::uint8_t>(c->type());
        ci.nullable = c->nullable();
        info.columns.push_back(std::move(ci));
    }
}

void fill_metadata(const ::Header* hdr, FileInfo& info) {
    info.features_count = hdr->features_count();
    info.index_node_size = hdr->index_node_size();

    // Transform = { Vector scale; Vector translate; }, Vector = 3 doubles.
    // Read via memcpy: see read_f64_at() for why the accessors are unsafe.
    if (const auto* t = hdr->transform()) {
        info.has_transform = true;
        info.scale = {read_f64_at(t, 0), read_f64_at(t, 8), read_f64_at(t, 16)};
        info.translate = {read_f64_at(t, 24), read_f64_at(t, 32), read_f64_at(t, 40)};
    }

    // GeographicalExtent = { Vector min; Vector max; }.
    if (const auto* e = hdr->geographical_extent()) {
        info.has_extent = true;
        info.geographical_extent = {read_f64_at(e, 0),  read_f64_at(e, 8),
                                    read_f64_at(e, 16), read_f64_at(e, 24),
                                    read_f64_at(e, 32), read_f64_at(e, 40)};
    }

    if (const auto* rs = hdr->reference_system()) {
        const std::string authority =
            rs->authority() != nullptr ? rs->authority()->str() : std::string("EPSG");
        if (rs->code() != 0) {
            info.crs = authority + ":" + std::to_string(rs->code());
        } else if (rs->code_string() != nullptr) {
            info.crs = authority + ":" + rs->code_string()->str();
        }
    }

    if (hdr->version() != nullptr) info.cityjson_version = hdr->version()->str();
    if (hdr->identifier() != nullptr) info.identifier = hdr->identifier()->str();
    if (hdr->title() != nullptr) info.title = hdr->title()->str();
}

/// Sum the attribute index lengths and record each index's absolute start.
/// Entries are walked in ascending column index, because that is the order
/// the writer concatenated the blobs in (writer/mod.rs:190-195).
std::uint64_t collect_attr_indices(const ::Header* hdr,
                                   std::vector<AttrIndexInfo>& out) {
    const auto* ais = hdr->attribute_index();
    if (ais == nullptr) return 0;

    out.reserve(ais->size());
    for (const auto* ai : *ais) {
        if (ai == nullptr) continue;
        AttrIndexInfo info{};
        info.column_index = ai->index();
        info.length = ai->length();
        info.branching_factor = ai->branching_factor();
        info.num_unique_items = ai->num_unique_items();
        info.begin = 0;  // filled once the layout is known
        out.push_back(info);
    }

    std::sort(out.begin(), out.end(),
              [](const AttrIndexInfo& a, const AttrIndexInfo& b) {
                  return a.column_index < b.column_index;
              });

    // Two indexes claiming the same column makes the cumulative-offset walk
    // ambiguous: there is no way to know which blob comes first.
    for (std::size_t i = 1; i < out.size(); ++i) {
        if (out[i].column_index == out[i - 1].column_index) {
            throw Error(ErrorCode::AttributeIndexNotFound,
                        "duplicate attribute index for column " +
                            std::to_string(out[i].column_index));
        }
    }

    std::uint64_t total = 0;
    for (const auto& ai : out) {
        total = detail::checked_add(total, ai.length, "attr index total");
    }
    return total;
}

}  // namespace

HeaderView read_header(std::shared_ptr<RangeReader> reader) {
    if (reader == nullptr) {
        throw Error(ErrorCode::IoError, "read_header: null reader");
    }
    const std::uint64_t total_size = reader->total_size();

    // Per-query buffering. 12944 = 2024 assumed header + the top 3 R-tree
    // levels ((1 + 16 + 256) * 40), matching http_reader/mod.rs:80-98, so a
    // remote open costs one range request rather than several.
    BufferedRangeReader buffered(std::move(reader), 12944);

    auto magic = buffered.read(0, kMagicBytesSize);
    if (magic.size() < kMagicBytesSize || !check_magic_bytes(bytes_view(magic))) {
        throw Error(ErrorCode::MissingMagicBytes, "not a FlatCityBuf file");
    }

    auto size_bytes = buffered.read(kMagicBytesSize, kHeaderSizeSize);
    if (size_bytes.size() < kHeaderSizeSize) {
        throw Error(ErrorCode::IllegalHeaderSize, "truncated before header size");
    }
    const std::uint32_t header_size = read_u32_le(size_bytes, 0);
    if (header_size < kHeaderMinBufferSize || header_size > kHeaderMaxBufferSize) {
        throw Error(ErrorCode::IllegalHeaderSize,
                    "illegal header size: " + std::to_string(header_size));
    }

    // The buffer handed to FlatBuffers MUST include the 4-byte size prefix:
    // header_size is that prefix, not a bespoke length field.
    //
    // ALIGNMENT: the schema contains 8-byte-aligned structs (Transform,
    // GeographicalExtent -- vectors of doubles). A size-prefixed buffer puts
    // the FlatBuffer body 4 bytes into the allocation, so if the buffer
    // started 8-aligned the body would land at 4 mod 8 and every such struct
    // would be misaligned -- confirmed by UBSan ("member call on misaligned
    // address ... for type 'GeographicalExtent'"). std::vector's data() is
    // at least 8-aligned, so we prepend kBodyAlignPad=4 bytes: the prefix
    // then sits at data()+4 and the body at data()+8, correctly aligned.
    const std::uint64_t want = kHeaderSizeSize + static_cast<std::uint64_t>(header_size);
    auto raw_buf = buffered.read(kMagicBytesSize, want);
    if (raw_buf.size() < want) {
        throw Error(ErrorCode::IllegalHeaderSize, "truncated header");
    }
    std::vector<std::uint8_t> buf(kBodyAlignPad + raw_buf.size());
    std::copy(raw_buf.begin(), raw_buf.end(), buf.begin() + kBodyAlignPad);

    // Structural verification with check_alignment disabled.
    //
    // Buffers written by the Rust implementation fail the C++ verifier's
    // alignment check while passing Rust's own verifier, and they read
    // correctly here. Verified empirically: the check fails for both the
    // size-prefixed and plain root forms, at every possible buffer start
    // residue mod 8, so it is the buffer's internal layout that C++
    // disagrees with -- not where we happen to place it. Every other check
    // (max_tables, max_depth, nested buffers) passes.
    //
    // Disabling only this check keeps the security-critical verification:
    // offsets, bounds, vector lengths and required fields are all still
    // enforced, which is what protects against malicious input. See the
    // UBSan test run in the suite, which confirms real field accesses are
    // not misaligned in practice.
    //
    // TODO(upstream): file against fcb_core -- the Rust writer should emit
    // buffers that satisfy the C++ verifier, since the format is meant to
    // be cross-implementation.
    flatbuffers::Verifier::Options opts;
    opts.check_alignment = false;
    flatbuffers::Verifier verifier(buf.data() + kBodyAlignPad,
                                  buf.size() - kBodyAlignPad, opts);
    if (!VerifySizePrefixedHeaderBuffer(verifier)) {
        throw Error(ErrorCode::InvalidFlatbuffer, "header failed FlatBuffers verification");
    }

    HeaderView view;
    view.buffer_ = std::make_shared<const std::vector<std::uint8_t>>(std::move(buf));

    const ::Header* hdr = view.raw();
    fill_metadata(hdr, view.info_);
    fill_columns(hdr, view.info_);

    const std::uint64_t attr_index_size =
        collect_attr_indices(hdr, view.attr_indices_);

    view.layout_ = compute_layout(header_size, view.info_.features_count,
                                  view.info_.index_node_size, attr_index_size);
    validate_layout_against_size(view.layout_, total_size);

    std::uint64_t cursor = view.layout_.attr_index_begin;
    for (auto& ai : view.attr_indices_) {
        ai.begin = cursor;
        cursor = detail::checked_add(cursor, ai.length, "attr index begin");
    }

    return view;
}

}  // namespace fcb
