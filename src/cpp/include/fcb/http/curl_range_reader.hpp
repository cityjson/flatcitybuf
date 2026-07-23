#pragma once

#ifdef FCB_WITH_CURL

#    include <fcb/error.hpp>
#    include <fcb/range_reader.hpp>

#    include <cstdint>
#    include <memory>
#    include <string>
#    include <vector>

namespace fcb {

struct CurlOptions {
    long timeout_ms = 30000;
    long connect_timeout_ms = 10000;
    bool follow_redirects = true;
    std::string user_agent = "flatcitybuf-cpp/0.8";

    /// Require the server to prove the representation has not changed
    /// between requests (ETag/If-Match, else Last-Modified). The core issues
    /// many ranges against one logical file and assumes they are the same
    /// bytes; without a validator a mutating URL silently mixes versions.
    /// Turn off only for sources known to be immutable.
    bool require_stable_representation = true;
};

/// HTTP range-request adapter, opt-in via the FCB_WITH_CURL CMake option.
///
/// libcurl is used rather than a header-only HTTP client specifically
/// because it brings its own platform TLS (Schannel / SecureTransport /
/// system OpenSSL), so this library never link-depends on a TLS stack. That
/// is what keeps the vcpkg port acceptable.
class CurlRangeReader : public RangeReader {
  public:
    explicit CurlRangeReader(const std::string& url, CurlOptions options = {});
    ~CurlRangeReader() override;

    CurlRangeReader(const CurlRangeReader&) = delete;
    CurlRangeReader& operator=(const CurlRangeReader&) = delete;

    std::uint64_t total_size() override;
    std::vector<std::uint8_t> read(std::uint64_t offset, std::uint64_t length) override;

    /// Number of HTTP requests issued. Exists so tests can assert on the
    /// prefetch behaviour that justifies the buffering design.
    std::uint64_t request_count() const { return request_count_; }
    void reset_request_count() { request_count_ = 0; }

  private:
    struct Impl;
    std::unique_ptr<Impl> impl_;
    std::uint64_t request_count_ = 0;
};

}  // namespace fcb

#endif  // FCB_WITH_CURL
