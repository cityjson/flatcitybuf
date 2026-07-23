#pragma once

#include <cstddef>
#include <cstdint>
#include <type_traits>
#include <vector>

namespace fcb {

/// Minimal C++17 stand-in for std::span: a non-owning view over contiguous
/// memory. The library targets C++17 because the GIS ecosystem consuming it
/// still ships C++17 compilers, so std::span is not available.
template <typename T> class span {
  public:
    span() noexcept : data_(nullptr), size_(0) {}
    span(T* data, std::size_t size) noexcept : data_(data), size_(size) {}

    /// Implicit view over a const vector, for span<const U>.
    template <typename U, typename = std::enable_if_t<std::is_same<const U, T>::value>>
    span(const std::vector<U>& v) noexcept : data_(v.data()), size_(v.size()) {}

    /// Implicit view over a mutable vector.
    span(std::vector<std::remove_const_t<T>>& v) noexcept : data_(v.data()), size_(v.size()) {}

    T* data() const noexcept { return data_; }
    std::size_t size() const noexcept { return size_; }
    bool empty() const noexcept { return size_ == 0; }

    T& operator[](std::size_t i) const noexcept { return data_[i]; }
    T* begin() const noexcept { return data_; }
    T* end() const noexcept { return data_ + size_; }

    span subspan(std::size_t offset, std::size_t count) const noexcept {
        return span(data_ + offset, count);
    }

  private:
    T* data_;
    std::size_t size_;
};

/// The workhorse alias: a read-only view over bytes.
using bytes_view = span<const std::uint8_t>;

}  // namespace fcb
