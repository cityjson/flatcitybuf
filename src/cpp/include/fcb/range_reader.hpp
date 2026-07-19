#pragma once

#include <cstdint>
#include <fstream>
#include <memory>
#include <string>
#include <vector>

#include <fcb/error.hpp>

namespace fcb {

/// One range in a batched read. `data` is filled in place by read_batch().
struct RangeRequest {
    std::uint64_t offset;
    std::uint64_t length;
    std::vector<std::uint8_t> data;
};

/// Synchronous byte-range source. Implement this to plug in any transport
/// (file, HTTP, memory, engine VFS). The core never assumes asynchrony;
/// batching, not async, is the concurrency primitive. This is the whole
/// point of the native port: a blocking interface is trivially wrapped by
/// any host application's threading model, whereas forcing our async model
/// on callers is exactly the tokio-over-FFI problem we are escaping.
///
/// CONTRACT -- implementors must honour all of it:
///
///  * read(offset, length) returns EXACTLY `length` bytes unless the range
///    crosses the end of the resource, in which case it returns exactly the
///    bytes that exist (possibly zero). It must never return a short buffer
///    for any other reason; a truncated network response is an error, not a
///    short read, and must throw fcb::Error{HttpError}.
///  * offset >= total_size() returns empty; that is not an error.
///  * length == 0 returns empty WITHOUT contacting the transport.
///  * Errors are reported by throwing fcb::Error. Returning garbage is not
///    an option -- the core cannot distinguish it from data.
///  * read_batch fills every element's `data` in place. Request ORDER IS
///    PRESERVED: the i-th request's bytes land in the i-th element. An
///    implementation may reorder its internal fetches freely.
///  * Partial batch failure is all-or-nothing: if any request cannot be
///    satisfied, read_batch throws and the caller must not inspect `data`.
///  * The resource must be STABLE for the reader's lifetime. The core
///    issues many ranges against one logical file and assumes they come
///    from the same bytes; HTTP implementations must pin the representation.
///  * THREAD SAFETY: instances are NOT thread-safe. One RangeReader serves
///    one query at a time; concurrent queries need separate instances.
///  * There is no cancellation mechanism. A transport needing one should
///    implement it out-of-band (e.g. a flag its read() checks and throws on).
class RangeReader {
public:
    virtual ~RangeReader() = default;

    /// Total byte length of the resource.
    ///
    /// This is a REQUIRED bounds/security contract, not a convenience: every
    /// computed section offset and range request is validated against it, so
    /// a corrupt header cannot make the core read or allocate out of bounds.
    /// (It is NOT needed to size the last feature -- every feature carries
    /// its own 4-byte size prefix.)
    virtual std::uint64_t total_size() = 0;

    /// Read `length` bytes at `offset`, subject to the contract above.
    virtual std::vector<std::uint8_t> read(std::uint64_t offset,
                                           std::uint64_t length) = 0;

    /// Fill every request, preserving order. Transports that can pipeline or
    /// multiplex should override this; the default is a sequential loop.
    virtual void read_batch(std::vector<RangeRequest>& requests);
};

/// Local-file adapter.
class FileRangeReader : public RangeReader {
public:
    explicit FileRangeReader(const std::string& path);

    std::uint64_t total_size() override;
    std::vector<std::uint8_t> read(std::uint64_t offset, std::uint64_t length) override;

private:
    std::string path_;
    std::ifstream stream_;
    std::uint64_t size_;
};

/// Caching decorator: over-fetches to `min_req_size` and serves subsequent
/// reads inside the cached window without touching the inner reader. This is
/// what makes HTTP traversal cheap while leaving file traversal unchanged.
///
/// OWNERSHIP: this is a PER-QUERY object. Each query (read_header,
/// select_all, select_bbox, select_attr) constructs its own with the
/// min_req_size appropriate to its phase, and discards it when done. Never
/// wrap once and mutate the window size later -- that makes concurrent
/// iterators silently alter each other's buffering policy and invites a
/// decorator wrapping a decorator. Hence there is no set_min_req_size.
class BufferedRangeReader : public RangeReader {
public:
    BufferedRangeReader(std::shared_ptr<RangeReader> inner, std::uint64_t min_req_size);

    std::uint64_t total_size() override;
    std::vector<std::uint8_t> read(std::uint64_t offset, std::uint64_t length) override;
    void read_batch(std::vector<RangeRequest>& requests) override;

private:
    /// Checked: an overflowing offset+length must not wrap into a false
    /// cache hit, which would then build invalid iterators when slicing.
    bool covers(std::uint64_t offset, std::uint64_t length) const;
    /// Over-fetch size, clamped to the resource so it cannot overflow.
    std::uint64_t clamped_fetch(std::uint64_t offset, std::uint64_t length);
    std::vector<std::uint8_t> slice_from_buffer(std::uint64_t offset,
                                                std::uint64_t length) const;

    std::shared_ptr<RangeReader> inner_;
    std::uint64_t min_req_size_;
    std::uint64_t buf_offset_ = 0;
    std::vector<std::uint8_t> buf_;
};

}  // namespace fcb
