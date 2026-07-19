#include <fcb/reader.hpp>

#include <fcb/packed_rtree.hpp>

#include "detail/checked.hpp"
#include "detail/feature_access.hpp"

#include <fcb/generated/feature_generated.h>

#include <algorithm>
#include <cstring>
#include <utility>

namespace fcb {

/// Leading padding so the size-prefixed FlatBuffer body ends up 8-aligned.
/// Same reasoning as in header.cpp: the 4-byte size prefix would otherwise
/// push the body to 4 mod 8 and misalign every 8-byte struct inside it.
static constexpr std::size_t kBodyAlignPad = 4;

// -------------------------------------------------------------- Feature ---

Feature::Feature(std::shared_ptr<const std::vector<std::uint8_t>> buffer,
                 std::uint64_t byte_offset,
                 std::size_t body_offset)
    : buffer_(std::move(buffer)), byte_offset_(byte_offset), body_offset_(body_offset) {}

const ::CityFeature* Feature::raw() const {
    if (buffer_ == nullptr) return nullptr;
    return GetSizePrefixedCityFeature(buffer_->data() + body_offset_);
}

const ::CityFeature* detail::FeatureAccess::get(const Feature& f) { return f.raw(); }

std::string Feature::id() const {
    const ::CityFeature* cf = raw();
    if (cf == nullptr || cf->id() == nullptr) return {};
    return cf->id()->str();
}

namespace {
const ::CityObject* object_at(const ::CityFeature* cf, std::size_t i) {
    if (cf == nullptr || cf->objects() == nullptr) return nullptr;
    if (i >= cf->objects()->size()) return nullptr;
    return cf->objects()->Get(static_cast<flatbuffers::uoffset_t>(i));
}
}  // namespace

bytes_view Feature::object_attributes(std::size_t i) const {
    const auto* obj = object_at(raw(), i);
    if (obj == nullptr || obj->attributes() == nullptr) return {};
    const auto* a = obj->attributes();
    return bytes_view(a->data(), a->size());
}

std::vector<ColumnInfo> Feature::object_columns(std::size_t i) const {
    std::vector<ColumnInfo> out;
    const auto* obj = object_at(raw(), i);
    if (obj == nullptr || obj->columns() == nullptr) return out;
    out.reserve(obj->columns()->size());
    for (const auto* c : *obj->columns()) {
        if (c == nullptr) continue;
        ColumnInfo ci{};
        ci.index = c->index();
        ci.name = c->name() != nullptr ? c->name()->str() : std::string();
        ci.type = static_cast<std::uint8_t>(c->type());
        ci.nullable = c->nullable();
        out.push_back(std::move(ci));
    }
    return out;
}

std::string Feature::object_id(std::size_t i) const {
    const auto* obj = object_at(raw(), i);
    if (obj == nullptr || obj->id() == nullptr) return {};
    return obj->id()->str();
}

std::size_t Feature::city_object_count() const {
    const ::CityFeature* cf = raw();
    if (cf == nullptr || cf->objects() == nullptr) return 0;
    return cf->objects()->size();
}

// ------------------------------------------------------- FeatureIterator ---

FeatureIterator::FeatureIterator(std::shared_ptr<RangeReader> reader,
                                 HeaderView header,
                                 IterationMode mode,
                                 std::vector<SearchResultItem> hits)
    : reader_(std::move(reader)),
      header_(std::move(header)),
      mode_(mode),
      hits_(std::move(hits)) {
    cursor_ = header_.layout().feature_begin;
}

bool FeatureIterator::next() {
    const std::uint64_t features_count = header_.info().features_count;
    const std::uint64_t total_size = reader_->total_size();

    std::uint64_t at = 0;
    if (mode_ == IterationMode::SequentialScan) {
        if (produced_ >= features_count) {
            current_ = Feature();
            return false;
        }
        at = cursor_;
    } else {
        if (hit_index_ >= hits_.size()) {
            current_ = Feature();
            return false;
        }
        at = detail::checked_add(header_.layout().feature_begin,
                                 hits_[hit_index_].offset, "feature offset");
        ++hit_index_;
    }

    auto prefix = reader_->read(at, 4);
    if (prefix.size() < 4) {
        // Reaching EOF before features_count features is a TRUNCATED file,
        // not a clean end of iteration. Accepting it silently would let a
        // file cut in half read as a valid short file.
        throw Error(ErrorCode::IoError,
                    "truncated feature section: expected " +
                        std::to_string(features_count) + " features, got " +
                        std::to_string(produced_));
    }

    const std::uint32_t len = static_cast<std::uint32_t>(prefix[0]) |
                              (static_cast<std::uint32_t>(prefix[1]) << 8) |
                              (static_cast<std::uint32_t>(prefix[2]) << 16) |
                              (static_cast<std::uint32_t>(prefix[3]) << 24);

    // Bound the allocation BEFORE making it: a crafted 0xFFFFFFFF prefix
    // would otherwise ask for ~4 GiB.
    if (len == 0 || len > kMaxFeatureSize) {
        throw Error(ErrorCode::InvalidFlatbuffer,
                    "implausible feature size: " + std::to_string(len));
    }
    const std::uint64_t want = detail::checked_add(4, len, "feature length");
    detail::require_within(at, want, total_size, "feature body");

    auto raw_buf = reader_->read(at, want);
    if (raw_buf.size() < want) {
        throw Error(ErrorCode::IoError, "truncated feature body");
    }

    auto buf = std::make_shared<std::vector<std::uint8_t>>(kBodyAlignPad + raw_buf.size());
    std::copy(raw_buf.begin(), raw_buf.end(), buf->begin() + kBodyAlignPad);

    // check_alignment is disabled for the same reason as the header: the
    // Rust writer emits internally misaligned structs, so the check can
    // never pass. All other structural verification still runs.
    flatbuffers::Verifier::Options opts;
    opts.check_alignment = false;
    flatbuffers::Verifier verifier(buf->data() + kBodyAlignPad,
                                   buf->size() - kBodyAlignPad, opts);
    if (!VerifySizePrefixedCityFeatureBuffer(verifier)) {
        throw Error(ErrorCode::InvalidFlatbuffer,
                    "feature failed FlatBuffers verification at offset " +
                        std::to_string(at));
    }

    current_ = Feature(std::const_pointer_cast<const std::vector<std::uint8_t>>(buf),
                       at - header_.layout().feature_begin, kBodyAlignPad);

    if (mode_ == IterationMode::SequentialScan) {
        cursor_ = detail::checked_add(at, want, "feature cursor");
    }
    ++produced_;
    return true;
}

// ------------------------------------------------------------ FcbReader ---

FcbReader::FcbReader(std::shared_ptr<RangeReader> reader, HeaderView header)
    : reader_(std::move(reader)), header_(std::move(header)) {}

FcbReader FcbReader::open_file(const std::string& path) {
    return open(std::make_shared<FileRangeReader>(path));
}

FcbReader FcbReader::open(std::shared_ptr<RangeReader> reader) {
    HeaderView header = read_header(reader);
    return FcbReader(std::move(reader), std::move(header));
}

FeatureIterator FcbReader::select_bbox(const BBox& query) {
    const auto& info = header_.info();
    const auto& layout = header_.layout();

    if (layout.rtree_size == 0 || info.features_count == 0) {
        throw Error(ErrorCode::NoIndex, "file has no spatial index");
    }

    // Index traversal gets its own buffering window. The Rust HTTP reader
    // coalesces node ranges up to 256 KB (http_reader/mod.rs:213); a window
    // of that size gives the same effect through the decorator.
    auto index_reader = std::make_shared<BufferedRangeReader>(reader_, 256 * 1024);
    auto hits = rtree_search_bbox(*index_reader, layout.rtree_begin,
                                  info.features_count, info.index_node_size, query);

    auto feature_reader = std::make_shared<BufferedRangeReader>(reader_, 1048576);
    return FeatureIterator(std::move(feature_reader), header_,
                           IterationMode::OffsetList, std::move(hits));
}

FeatureIterator FcbReader::select_all() {
    // Per-query buffering at the feature-phase window size, matching
    // DEFAULT_HTTP_FETCH_SIZE in http_reader/mod.rs:42. Constructed fresh
    // per query so concurrent iterators cannot disturb each other.
    auto buffered = std::make_shared<BufferedRangeReader>(reader_, 1048576);
    return FeatureIterator(std::move(buffered), header_,
                           IterationMode::SequentialScan, {});
}

}  // namespace fcb
