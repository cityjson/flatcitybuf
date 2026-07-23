// Query features by ATTRIBUTE, using the static B+tree index.
//
//     query_attributes <file.fcb> <field> <eq|ne|gt|ge|lt|le> <value> [...]
//
// Several conditions are AND-intersected:
//     query_attributes delft.fcb b3_h_dak_50p gt 20 b3_dak_type eq "slanted"
//
// The subtlety this example exists to show: the value you compare against is
// a typed `KeyValue`, not a string, and its type must match the column's type
// on disk. Getting that wrong does not throw -- the bytes are reinterpreted
// and you get plausible garbage. So rather than hardcoding a factory, this
// looks the column up in the header and dispatches on its declared type.
// Run inspect_header.cpp first to see which columns are queryable at all.
#include <fcb/generated/header_generated.h>
#include <fcb/header.hpp>
#include <fcb/reader.hpp>
#include <fcb/stree.hpp>

#include <cstdio>
#include <cstdlib>
#include <optional>
#include <string>
#include <vector>

namespace {

std::optional<fcb::Operator> parse_op(const std::string& s) {
    if (s == "eq")
        return fcb::Operator::Eq;
    if (s == "ne")
        return fcb::Operator::Ne;
    if (s == "gt")
        return fcb::Operator::Gt;
    if (s == "ge")
        return fcb::Operator::Ge;
    if (s == "lt")
        return fcb::Operator::Lt;
    if (s == "le")
        return fcb::Operator::Le;
    return std::nullopt;
}

const fcb::ColumnInfo* find_column(const fcb::FileInfo& info, const std::string& name) {
    for (const fcb::ColumnInfo& c : info.columns) {
        if (c.name == name)
            return &c;
    }
    return nullptr;
}

/// Build a KeyValue of the column's own type from the text on the command
/// line. String columns are indexed as fixed-width keys; 50 bytes is what the
/// writer uses for String, 100 for Json/Binary.
fcb::KeyValue make_value(const fcb::ColumnInfo& col, const std::string& text) {
    switch (static_cast<::ColumnType>(col.type)) {
        case ::ColumnType::Byte:
            return fcb::KeyValue::from_i8(static_cast<std::int8_t>(std::stoi(text)));
        case ::ColumnType::UByte:
            return fcb::KeyValue::from_u8(static_cast<std::uint8_t>(std::stoul(text)));
        case ::ColumnType::Bool:
            return fcb::KeyValue::from_bool(text == "true" || text == "1");
        case ::ColumnType::Short:
            return fcb::KeyValue::from_i16(static_cast<std::int16_t>(std::stoi(text)));
        case ::ColumnType::UShort:
            return fcb::KeyValue::from_u16(static_cast<std::uint16_t>(std::stoul(text)));
        case ::ColumnType::Int:
            return fcb::KeyValue::from_i32(static_cast<std::int32_t>(std::stol(text)));
        case ::ColumnType::UInt:
            return fcb::KeyValue::from_u32(static_cast<std::uint32_t>(std::stoul(text)));
        case ::ColumnType::Long:
            return fcb::KeyValue::from_i64(std::stoll(text));
        case ::ColumnType::ULong:
            return fcb::KeyValue::from_u64(std::stoull(text));
        case ::ColumnType::Float:
            return fcb::KeyValue::from_f32(std::stof(text));
        case ::ColumnType::Double:
            return fcb::KeyValue::from_f64(std::stod(text));
        case ::ColumnType::String:
            return fcb::KeyValue::from_string(fcb::KeyKind::String50, text);
        case ::ColumnType::Json:
        case ::ColumnType::Binary:
            return fcb::KeyValue::from_string(fcb::KeyKind::String100, text);
        default:
            throw fcb::Error(fcb::ErrorCode::UnsupportedColumnType,
                             "column '" + col.name + "' has a type this example cannot parse");
    }
}

}  // namespace

int main(int argc, char** argv) {
    if (argc < 5 || (argc - 2) % 3 != 0) {
        std::fprintf(
            stderr,
            "usage: %s <file.fcb> <field> <eq|ne|gt|ge|lt|le> <value> [field op value]...\n",
            argv[0]);
        return 2;
    }
    try {
        fcb::FcbReader reader = fcb::FcbReader::open_file(argv[1]);
        const fcb::FileInfo& info = reader.header().info();

        fcb::AttrQuery query;
        for (int i = 2; i + 2 < argc; i += 3) {
            const std::string field = argv[i];
            const std::optional<fcb::Operator> op = parse_op(argv[i + 1]);
            if (!op.has_value()) {
                std::fprintf(stderr, "unknown operator '%s'\n", argv[i + 1]);
                return 2;
            }
            const fcb::ColumnInfo* col = find_column(info, field);
            if (col == nullptr) {
                std::fprintf(stderr, "no column named '%s' in this file\n", field.c_str());
                return 2;
            }
            query.push_back({field, *op, make_value(*col, argv[i + 2])});
            std::fprintf(stderr, "condition: %s %s %s (column type %s)\n", field.c_str(),
                         argv[i + 1], argv[i + 2],
                         EnumNameColumnType(static_cast<::ColumnType>(col->type)));
        }

        // Default options VERIFY each candidate against the decoded attribute.
        // That matters for string columns: the index stores keys truncated to
        // 50 (or 100) bytes, so it answers with candidates, not answers. Pass
        // {true} to skip verification -- faster, and wrong for long strings.
        fcb::FeatureIterator it = reader.select_attr(query, fcb::AttrQueryOptions{});

        unsigned long long matches = 0;
        while (it.next()) {
            std::printf("%s\n", it.current().id().c_str());
            ++matches;
        }
        std::fprintf(stderr, "%llu of %llu features matched\n", matches,
                     static_cast<unsigned long long>(info.features_count));
        return 0;
    } catch (const fcb::Error& e) {
        // AttributeIndexNotFound is the one worth calling out: the column
        // exists but was never indexed, so it cannot be queried -- only read.
        if (e.code() == fcb::ErrorCode::AttributeIndexNotFound) {
            std::fprintf(stderr,
                         "error: %s\n"
                         "hint: that column has no B+tree index. Run inspect_header to see\n"
                         "      which columns are queryable, or re-serialize with -a/-A.\n",
                         e.what());
        } else {
            std::fprintf(stderr, "error: %s\n", e.what());
        }
        return 1;
    }
}
