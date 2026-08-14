#include <fcb/generated/header_generated.h>
#include <fcb/key.hpp>

#include <algorithm>
#include <cmath>
#include <cstring>
#include <limits>

namespace fcb {

namespace {

bool is_string_kind(KeyKind k) {
    return k == KeyKind::String20 || k == KeyKind::String50 || k == KeyKind::String100;
}

template <typename T> void put_le(std::vector<std::uint8_t>& out, T value) {
    static_assert(std::is_integral<T>::value, "integral only");
    using U = typename std::make_unsigned<T>::type;
    U u = static_cast<U>(value);
    for (std::size_t i = 0; i < sizeof(T); ++i) {
        out.push_back(static_cast<std::uint8_t>((u >> (8 * i)) & 0xFF));
    }
}

template <typename T> T get_le(bytes_view b, std::size_t at = 0) {
    using U = typename std::make_unsigned<T>::type;
    U u = 0;
    for (std::size_t i = 0; i < sizeof(T); ++i) {
        u |= static_cast<U>(b[at + i]) << (8 * i);
    }
    return static_cast<T>(u);
}

/// ordered_float total order: NaN equals itself and sorts above everything.
/// Do NOT use std::less -- NaN would break the tree's ordering invariants.
int cmp_ordered_double(double a, double b) {
    const bool na = std::isnan(a);
    const bool nb = std::isnan(b);
    if (na && nb)
        return 0;
    if (na)
        return 1;
    if (nb)
        return -1;
    if (a < b)
        return -1;
    if (a > b)
        return 1;
    return 0;  // also covers -0.0 == +0.0
}

void require_size(bytes_view b, std::size_t need, const char* what) {
    if (b.size() < need) {
        throw Error(ErrorCode::InvalidAttributeValue, std::string("short key buffer for ") + what);
    }
}

}  // namespace

std::size_t key_serialized_size(KeyKind kind) {
    switch (kind) {
        case KeyKind::Int8:
        case KeyKind::UInt8:
        case KeyKind::Bool:
            return 1;
        case KeyKind::Int16:
        case KeyKind::UInt16:
            return 2;
        case KeyKind::Int32:
        case KeyKind::UInt32:
        case KeyKind::Float32:
            return 4;
        case KeyKind::Int64:
        case KeyKind::UInt64:
        case KeyKind::Float64:
            return 8;
        case KeyKind::DateTime:
            return 12;  // i64 seconds + u32 nanos
        case KeyKind::String20:
            return 20;
        case KeyKind::String50:
            return 50;
        case KeyKind::String100:
            return 100;
    }
    throw Error(ErrorCode::UnsupportedColumnType, "unknown key kind");
}

#define FCB_KV_INT(NAME, TYPE, KIND, FIELD)          \
    KeyValue KeyValue::NAME(TYPE v) {                \
        KeyValue k;                                  \
        k.kind_ = KeyKind::KIND;                     \
        k.FIELD = static_cast<decltype(k.FIELD)>(v); \
        return k;                                    \
    }

FCB_KV_INT(from_i8, std::int8_t, Int8, i_)
FCB_KV_INT(from_u8, std::uint8_t, UInt8, u_)
FCB_KV_INT(from_i16, std::int16_t, Int16, i_)
FCB_KV_INT(from_u16, std::uint16_t, UInt16, u_)
FCB_KV_INT(from_i32, std::int32_t, Int32, i_)
FCB_KV_INT(from_u32, std::uint32_t, UInt32, u_)
FCB_KV_INT(from_i64, std::int64_t, Int64, i_)
FCB_KV_INT(from_u64, std::uint64_t, UInt64, u_)
#undef FCB_KV_INT

KeyValue KeyValue::from_f32(float v) {
    KeyValue k;
    k.kind_ = KeyKind::Float32;
    k.f_ = static_cast<double>(v);
    return k;
}

KeyValue KeyValue::from_f64(double v) {
    KeyValue k;
    k.kind_ = KeyKind::Float64;
    k.f_ = v;
    return k;
}

KeyValue KeyValue::from_bool(bool v) {
    KeyValue k;
    k.kind_ = KeyKind::Bool;
    k.u_ = v ? 1 : 0;
    return k;
}

KeyValue KeyValue::from_datetime(std::int64_t seconds, std::uint32_t nanos) {
    KeyValue k;
    k.kind_ = KeyKind::DateTime;
    k.i_ = seconds;
    k.u_ = nanos;
    return k;
}

KeyValue KeyValue::from_string(KeyKind kind, const std::string& v) {
    if (!is_string_kind(kind)) {
        throw Error(ErrorCode::UnsupportedColumnType, "from_string on a non-string kind");
    }
    KeyValue k;
    k.kind_ = kind;
    k.str_ = v;  // kept untruncated, for post-filtering
    return k;
}

std::vector<std::uint8_t> encode_key(const KeyValue& v) {
    std::vector<std::uint8_t> out;
    const std::size_t n = key_serialized_size(v.kind_);
    out.reserve(n);

    switch (v.kind_) {
        case KeyKind::Int8:
            put_le<std::int8_t>(out, static_cast<std::int8_t>(v.i_));
            break;
        case KeyKind::UInt8:
            put_le<std::uint8_t>(out, static_cast<std::uint8_t>(v.u_));
            break;
        case KeyKind::Int16:
            put_le<std::int16_t>(out, static_cast<std::int16_t>(v.i_));
            break;
        case KeyKind::UInt16:
            put_le<std::uint16_t>(out, static_cast<std::uint16_t>(v.u_));
            break;
        case KeyKind::Int32:
            put_le<std::int32_t>(out, static_cast<std::int32_t>(v.i_));
            break;
        case KeyKind::UInt32:
            put_le<std::uint32_t>(out, static_cast<std::uint32_t>(v.u_));
            break;
        case KeyKind::Int64:
            put_le<std::int64_t>(out, v.i_);
            break;
        case KeyKind::UInt64:
            put_le<std::uint64_t>(out, v.u_);
            break;
        case KeyKind::Bool:
            out.push_back(v.u_ != 0 ? 1 : 0);
            break;

        case KeyKind::Float32: {
            // Raw IEEE-754 bits, little-endian. NO order-preserving
            // transform: key.rs:323-345 writes the plain bit pattern, and
            // applying the usual sign-flip trick would disagree with every
            // file the reference has written.
            const float f = static_cast<float>(v.f_);
            std::uint32_t bits;
            std::memcpy(&bits, &f, sizeof(bits));
            put_le<std::uint32_t>(out, bits);
            break;
        }
        case KeyKind::Float64: {
            std::uint64_t bits;
            std::memcpy(&bits, &v.f_, sizeof(bits));
            put_le<std::uint64_t>(out, bits);
            break;
        }
        case KeyKind::DateTime:
            put_le<std::int64_t>(out, v.i_);
            put_le<std::uint32_t>(out, static_cast<std::uint32_t>(v.u_));
            break;

        case KeyKind::String20:
        case KeyKind::String50:
        case KeyKind::String100: {
            // Copy min(len, N) BYTES and zero-pad. Truncation is silent and
            // does not respect UTF-8 boundaries (key.rs:483-489), so two
            // distinct strings sharing an N-byte prefix become identical
            // here -- which is why select_attr must post-filter.
            out.assign(n, 0);
            const std::size_t take = std::min(v.str_.size(), n);
            std::memcpy(out.data(), v.str_.data(), take);
            break;
        }
    }
    return out;
}

KeyValue decode_key(KeyKind kind, bytes_view b) {
    const std::size_t n = key_serialized_size(kind);
    require_size(b, n, "decode_key");

    switch (kind) {
        case KeyKind::Int8:
            return KeyValue::from_i8(get_le<std::int8_t>(b));
        case KeyKind::UInt8:
            return KeyValue::from_u8(get_le<std::uint8_t>(b));
        case KeyKind::Int16:
            return KeyValue::from_i16(get_le<std::int16_t>(b));
        case KeyKind::UInt16:
            return KeyValue::from_u16(get_le<std::uint16_t>(b));
        case KeyKind::Int32:
            return KeyValue::from_i32(get_le<std::int32_t>(b));
        case KeyKind::UInt32:
            return KeyValue::from_u32(get_le<std::uint32_t>(b));
        case KeyKind::Int64:
            return KeyValue::from_i64(get_le<std::int64_t>(b));
        case KeyKind::UInt64:
            return KeyValue::from_u64(get_le<std::uint64_t>(b));
        case KeyKind::Bool:
            return KeyValue::from_bool(b[0] != 0);

        case KeyKind::Float32: {
            const std::uint32_t bits = get_le<std::uint32_t>(b);
            float f;
            std::memcpy(&f, &bits, sizeof(f));
            return KeyValue::from_f32(f);
        }
        case KeyKind::Float64: {
            const std::uint64_t bits = get_le<std::uint64_t>(b);
            double d;
            std::memcpy(&d, &bits, sizeof(d));
            return KeyValue::from_f64(d);
        }
        case KeyKind::DateTime:
            return KeyValue::from_datetime(get_le<std::int64_t>(b), get_le<std::uint32_t>(b, 8));

        case KeyKind::String20:
        case KeyKind::String50:
        case KeyKind::String100: {
            // Stop at the first NUL, as to_string_lossy does (key.rs:511).
            std::size_t len = 0;
            while (len < n && b[len] != 0)
                ++len;
            return KeyValue::from_string(kind,
                                         std::string(reinterpret_cast<const char*>(b.data()), len));
        }
    }
    throw Error(ErrorCode::UnsupportedColumnType, "unknown key kind");
}

int compare_keys(const KeyValue& a, const KeyValue& b) {
    if (a.kind_ != b.kind_) {
        throw Error(ErrorCode::QueryExecutionError, "comparing keys of different kinds");
    }
    switch (a.kind_) {
        case KeyKind::Int8:
        case KeyKind::Int16:
        case KeyKind::Int32:
        case KeyKind::Int64:
            return a.i_ < b.i_ ? -1 : (a.i_ > b.i_ ? 1 : 0);

        case KeyKind::UInt8:
        case KeyKind::UInt16:
        case KeyKind::UInt32:
        case KeyKind::UInt64:
        case KeyKind::Bool:
            return a.u_ < b.u_ ? -1 : (a.u_ > b.u_ ? 1 : 0);

        case KeyKind::Float32:
        case KeyKind::Float64:
            return cmp_ordered_double(a.f_, b.f_);

        case KeyKind::DateTime:
            if (a.i_ != b.i_)
                return a.i_ < b.i_ ? -1 : 1;
            return a.u_ < b.u_ ? -1 : (a.u_ > b.u_ ? 1 : 0);

        case KeyKind::String20:
        case KeyKind::String50:
        case KeyKind::String100: {
            // Compare the ENCODED (truncated, padded) forms, because that is
            // what the tree stores and orders by.
            const auto ea = encode_key(a);
            const auto eb = encode_key(b);
            const int c = std::memcmp(ea.data(), eb.data(), ea.size());
            return c < 0 ? -1 : (c > 0 ? 1 : 0);
        }
    }
    throw Error(ErrorCode::UnsupportedColumnType, "unknown key kind");
}

KeyValue key_min(KeyKind kind) {
    switch (kind) {
        case KeyKind::Int8:
            return KeyValue::from_i8(std::numeric_limits<std::int8_t>::min());
        case KeyKind::UInt8:
            return KeyValue::from_u8(0);
        case KeyKind::Int16:
            return KeyValue::from_i16(std::numeric_limits<std::int16_t>::min());
        case KeyKind::UInt16:
            return KeyValue::from_u16(0);
        case KeyKind::Int32:
            return KeyValue::from_i32(std::numeric_limits<std::int32_t>::min());
        case KeyKind::UInt32:
            return KeyValue::from_u32(0);
        case KeyKind::Int64:
            return KeyValue::from_i64(std::numeric_limits<std::int64_t>::min());
        case KeyKind::UInt64:
            return KeyValue::from_u64(0);
        case KeyKind::Float32:
            return KeyValue::from_f32(-std::numeric_limits<float>::infinity());
        case KeyKind::Float64:
            return KeyValue::from_f64(-std::numeric_limits<double>::infinity());
        case KeyKind::Bool:
            return KeyValue::from_bool(false);
        // Epoch 0, matching key.rs:242 -- NOT the true i64 minimum. Pre-1970
        // timestamps are therefore invisible to range queries, in both
        // implementations. Reproduced deliberately.
        case KeyKind::DateTime:
            return KeyValue::from_datetime(0, 0);
        case KeyKind::String20:
        case KeyKind::String50:
        case KeyKind::String100:
            return KeyValue::from_string(kind, std::string());
    }
    throw Error(ErrorCode::UnsupportedColumnType, "unknown key kind");
}

KeyValue key_max(KeyKind kind) {
    switch (kind) {
        case KeyKind::Int8:
            return KeyValue::from_i8(std::numeric_limits<std::int8_t>::max());
        case KeyKind::UInt8:
            return KeyValue::from_u8(std::numeric_limits<std::uint8_t>::max());
        case KeyKind::Int16:
            return KeyValue::from_i16(std::numeric_limits<std::int16_t>::max());
        case KeyKind::UInt16:
            return KeyValue::from_u16(std::numeric_limits<std::uint16_t>::max());
        case KeyKind::Int32:
            return KeyValue::from_i32(std::numeric_limits<std::int32_t>::max());
        case KeyKind::UInt32:
            return KeyValue::from_u32(std::numeric_limits<std::uint32_t>::max());
        case KeyKind::Int64:
            return KeyValue::from_i64(std::numeric_limits<std::int64_t>::max());
        case KeyKind::UInt64:
            return KeyValue::from_u64(std::numeric_limits<std::uint64_t>::max());
        // +inf, matching key.rs:139. NaN sorts ABOVE +inf in the total
        // order, so NaN-keyed features are excluded from range-lowered
        // operators (Ge, Ne). Reproduced deliberately so results match Rust.
        case KeyKind::Float32:
            return KeyValue::from_f32(std::numeric_limits<float>::infinity());
        case KeyKind::Float64:
            return KeyValue::from_f64(std::numeric_limits<double>::infinity());
        case KeyKind::Bool:
            return KeyValue::from_bool(true);
        case KeyKind::DateTime:
            return KeyValue::from_datetime(253402300799LL, 999999999U);
        case KeyKind::String20:
        case KeyKind::String50:
        case KeyKind::String100:
            return KeyValue::from_string(kind, std::string(key_serialized_size(kind), '\xFF'));
    }
    throw Error(ErrorCode::UnsupportedColumnType, "unknown key kind");
}

KeyKind key_kind_for_column(std::uint8_t column_type) {
    switch (static_cast<::ColumnType>(column_type)) {
        // Byte -> UInt8, deliberately. The writer stores Byte as u8
        // (writer/attribute.rs) and builds MemoryIndex<u8>
        // (writer/attr_index.rs), so it must be read back unsigned or every
        // stored value above 127 comes back negative. Rust now agrees on
        // both paths -- its index reader (reader/attr_query.rs) and its
        // value reader (reader/deserializer.rs) each decode u8 -- so this is
        // no longer a divergence.
        case ::ColumnType::Byte:
            return KeyKind::UInt8;
        case ::ColumnType::UByte:
            return KeyKind::UInt8;
        case ::ColumnType::Bool:
            return KeyKind::Bool;
        case ::ColumnType::Short:
            return KeyKind::Int16;
        case ::ColumnType::UShort:
            return KeyKind::UInt16;
        case ::ColumnType::Int:
            return KeyKind::Int32;
        case ::ColumnType::UInt:
            return KeyKind::UInt32;
        case ::ColumnType::Long:
            return KeyKind::Int64;
        case ::ColumnType::ULong:
            return KeyKind::UInt64;
        case ::ColumnType::Float:
            return KeyKind::Float32;
        case ::ColumnType::Double:
            return KeyKind::Float64;
        case ::ColumnType::String:
            return KeyKind::String50;
        case ::ColumnType::DateTime:
            return KeyKind::DateTime;
        case ::ColumnType::Json:
            return KeyKind::String100;
        case ::ColumnType::Binary:
            return KeyKind::String100;
    }
    throw Error(ErrorCode::UnsupportedColumnType, "unknown column type");
}

}  // namespace fcb
