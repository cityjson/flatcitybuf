#include <fcb/range_reader.hpp>

#include "detail/checked.hpp"

#include <algorithm>

namespace fcb {

void RangeReader::read_batch(std::vector<RangeRequest>& requests) {
    for (auto& r : requests) {
        r.data = read(r.offset, r.length);
    }
}

// ---------------------------------------------------------------- File ---

FileRangeReader::FileRangeReader(const std::string& path)
    : path_(path), stream_(path, std::ios::binary | std::ios::ate) {
    if (!stream_) {
        throw Error(ErrorCode::IoError, "cannot open file: " + path);
    }
    size_ = static_cast<std::uint64_t>(stream_.tellg());
}

std::uint64_t FileRangeReader::total_size() { return size_; }

std::vector<std::uint8_t> FileRangeReader::read(std::uint64_t offset,
                                                std::uint64_t length) {
    if (length == 0) return {};
    if (offset >= size_) return {};

    // Clamp to EOF: a range crossing the end returns exactly what exists.
    const std::uint64_t n = std::min<std::uint64_t>(length, size_ - offset);
    std::vector<std::uint8_t> out(static_cast<std::size_t>(n));

    stream_.clear();
    stream_.seekg(static_cast<std::streamoff>(offset), std::ios::beg);
    stream_.read(reinterpret_cast<char*>(out.data()), static_cast<std::streamsize>(n));
    if (stream_.gcount() != static_cast<std::streamsize>(n)) {
        throw Error(ErrorCode::IoError, "short read from " + path_);
    }
    return out;
}

// ------------------------------------------------------------ Buffered ---

BufferedRangeReader::BufferedRangeReader(std::shared_ptr<RangeReader> inner,
                                         std::uint64_t min_req_size)
    : inner_(std::move(inner)), min_req_size_(min_req_size) {}

std::uint64_t BufferedRangeReader::total_size() { return inner_->total_size(); }

bool BufferedRangeReader::covers(std::uint64_t offset, std::uint64_t length) const {
    if (buf_.empty() || offset < buf_offset_) return false;
    // Throws rather than wrapping; both ends derive from file-supplied values.
    return detail::range_end(offset, length) <=
           detail::range_end(buf_offset_, buf_.size());
}

std::vector<std::uint8_t> BufferedRangeReader::slice_from_buffer(
    std::uint64_t offset, std::uint64_t length) const {
    const std::uint64_t rel = offset - buf_offset_;
    return std::vector<std::uint8_t>(
        buf_.begin() + static_cast<std::ptrdiff_t>(rel),
        buf_.begin() + static_cast<std::ptrdiff_t>(rel + length));
}

std::vector<std::uint8_t> BufferedRangeReader::read(std::uint64_t offset,
                                                    std::uint64_t length) {
    if (length == 0) return {};  // contract: never contact the transport

    if (!covers(offset, length)) {
        const std::uint64_t fetch = std::max<std::uint64_t>(length, min_req_size_);
        buf_ = inner_->read(offset, fetch);
        buf_offset_ = offset;
    }

    const std::uint64_t rel = offset - buf_offset_;
    if (rel >= buf_.size()) return {};
    const std::uint64_t n = std::min<std::uint64_t>(length, buf_.size() - rel);
    return slice_from_buffer(offset, n);
}

void BufferedRangeReader::read_batch(std::vector<RangeRequest>& requests) {
    // Serve what the cache already covers and forward only the misses.
    // Blindly forwarding everything would defeat the decorator exactly when
    // tree traversal batches -- which is its whole reason to exist.
    struct Miss {
        std::size_t index;
        std::uint64_t offset;
        std::uint64_t want;
        std::uint64_t fetch;
    };
    std::vector<Miss> misses;
    misses.reserve(requests.size());

    for (std::size_t i = 0; i < requests.size(); ++i) {
        auto& r = requests[i];
        if (r.length == 0) {
            r.data.clear();
        } else if (covers(r.offset, r.length)) {
            r.data = slice_from_buffer(r.offset, r.length);
        } else {
            misses.push_back(Miss{i, r.offset, r.length,
                                  std::max<std::uint64_t>(r.length, min_req_size_)});
        }
    }
    if (misses.empty()) return;

    // Over-fetch each miss to min_req_size, exactly as read() does; otherwise
    // the cache seeded below would be one request wide and buy nothing.
    std::vector<RangeRequest> fetches;
    fetches.reserve(misses.size());
    for (const auto& m : misses) {
        fetches.push_back(RangeRequest{m.offset, m.fetch, {}});
    }

    inner_->read_batch(fetches);

    // Hand back only the bytes each caller asked for -- never the over-fetch.
    for (std::size_t k = 0; k < misses.size(); ++k) {
        const auto& m = misses[k];
        auto& got = fetches[k].data;
        const std::uint64_t n = std::min<std::uint64_t>(m.want, got.size());
        requests[m.index].data.assign(
            got.begin(), got.begin() + static_cast<std::ptrdiff_t>(n));
    }

    // Seed the single-window cache from the last over-fetched block:
    // traversal walks forward, so the most recent range is the likeliest hit.
    buf_offset_ = misses.back().offset;
    buf_ = std::move(fetches.back().data);
}

}  // namespace fcb
