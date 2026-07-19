#pragma once

#include <cstdint>
#include <string>

#include <fcb/error.hpp>

namespace fcb {
namespace detail {

// Every value these touch comes from the file and may be hostile or corrupt.
// Overflow must THROW, never wrap -- a wrapped size becomes an
// under-allocated buffer, which is how a length check turns into a heap
// overflow. Use these at every trust boundary, not just in layout.cpp.

inline std::uint64_t checked_add(std::uint64_t a, std::uint64_t b,
                                 const char* what = "add") {
    if (a > UINT64_MAX - b) {
        throw Error(ErrorCode::IllegalHeaderSize,
                    std::string("size arithmetic overflow (") + what + ")");
    }
    return a + b;
}

inline std::uint64_t checked_mul(std::uint64_t a, std::uint64_t b,
                                 const char* what = "mul") {
    if (a != 0 && b > UINT64_MAX / a) {
        throw Error(ErrorCode::IllegalHeaderSize,
                    std::string("size arithmetic overflow (") + what + ")");
    }
    return a * b;
}

/// ceil(a / b) without the (a + b - 1) overflow hazard. Throws on b == 0
/// rather than trapping, because callers pass file-supplied divisors.
inline std::uint64_t ceil_div(std::uint64_t a, std::uint64_t b) {
    if (b == 0) {
        throw Error(ErrorCode::IllegalHeaderSize, "division by zero in size arithmetic");
    }
    return a / b + (a % b != 0 ? 1 : 0);
}

/// End of a range, checked. Use at EVERY place that forms offset+length:
/// cache coverage tests, feature cursor advance, node slab bounds, payload
/// entry bounds, and HTTP Range header construction.
inline std::uint64_t range_end(std::uint64_t offset, std::uint64_t length) {
    return checked_add(offset, length, "range_end");
}

/// Throws unless [offset, offset+length) lies wholly within `limit`.
inline void require_within(std::uint64_t offset, std::uint64_t length,
                           std::uint64_t limit, const char* what) {
    if (range_end(offset, length) > limit) {
        throw Error(ErrorCode::IllegalHeaderSize,
                    std::string("range out of bounds: ") + what);
    }
}

}  // namespace detail
}  // namespace fcb
