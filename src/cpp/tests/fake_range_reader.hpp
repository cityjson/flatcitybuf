#pragma once

#include <fcb/range_reader.hpp>

#include <algorithm>
#include <cstdint>
#include <stdexcept>
#include <vector>

namespace fcb {
namespace testing {

/// In-memory RangeReader that records every request, so tests can assert on
/// IO behaviour (coalescing, prefetch, request counts) deterministically
/// without a network or filesystem.
class FakeRangeReader : public RangeReader {
  public:
    explicit FakeRangeReader(std::vector<std::uint8_t> data) : data_(std::move(data)) {}

    std::uint64_t total_size() override { return data_.size(); }

    std::vector<std::uint8_t> read(std::uint64_t offset, std::uint64_t length) override {
        requests.push_back({offset, length});
        if (length == 0)
            return {};
        if (offset >= data_.size())
            return {};
        const std::uint64_t end = std::min<std::uint64_t>(offset + length, data_.size());
        return std::vector<std::uint8_t>(data_.begin() + static_cast<std::ptrdiff_t>(offset),
                                         data_.begin() + static_cast<std::ptrdiff_t>(end));
    }

    struct Req {
        std::uint64_t offset;
        std::uint64_t length;
    };
    std::vector<Req> requests;

  private:
    std::vector<std::uint8_t> data_;
};

}  // namespace testing
}  // namespace fcb
