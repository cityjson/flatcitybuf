#include <fcb/http/curl_range_reader.hpp>

#ifdef FCB_WITH_CURL

#    include <algorithm>
#    include <cctype>
#    include <cstdlib>
#    include <cstring>

#    include <curl/curl.h>

#    include "../detail/checked.hpp"

namespace fcb {

namespace {

std::size_t write_cb(char* ptr, std::size_t size, std::size_t nmemb, void* userdata) {
    auto* out = static_cast<std::vector<std::uint8_t>*>(userdata);
    const std::size_t n = size * nmemb;
    out->insert(out->end(), reinterpret_cast<std::uint8_t*>(ptr),
                reinterpret_cast<std::uint8_t*>(ptr) + n);
    return n;
}

std::size_t header_cb(char* ptr, std::size_t size, std::size_t nmemb, void* userdata) {
    auto* headers = static_cast<std::vector<std::string>*>(userdata);
    headers->emplace_back(ptr, size * nmemb);
    return size * nmemb;
}

std::string lower(std::string s) {
    std::transform(s.begin(), s.end(), s.begin(),
                   [](unsigned char c) { return static_cast<char>(std::tolower(c)); });
    return s;
}

std::string find_header(const std::vector<std::string>& headers, const std::string& name) {
    const std::string want = lower(name) + ":";
    for (const auto& h : headers) {
        if (lower(h).rfind(want, 0) == 0) {
            std::string v = h.substr(want.size());
            // trim
            const auto b = v.find_first_not_of(" \t");
            const auto e = v.find_last_not_of(" \t\r\n");
            if (b == std::string::npos)
                return {};
            return v.substr(b, e - b + 1);
        }
    }
    return {};
}

/// Parse "bytes <start>-<end>/<total>". Returns false if malformed.
bool parse_content_range(const std::string& v, std::uint64_t& start, std::uint64_t& end,
                         std::uint64_t& total) {
    if (v.rfind("bytes ", 0) != 0)
        return false;
    const std::string rest = v.substr(6);
    const auto dash = rest.find('-');
    const auto slash = rest.find('/');
    if (dash == std::string::npos || slash == std::string::npos || slash < dash)
        return false;
    try {
        start = std::stoull(rest.substr(0, dash));
        end = std::stoull(rest.substr(dash + 1, slash - dash - 1));
        const std::string t = rest.substr(slash + 1);
        if (t == "*")
            return false;
        total = std::stoull(t);
    } catch (...) {
        return false;
    }
    return true;
}

}  // namespace

struct CurlRangeReader::Impl {
    std::string url;
    CurlOptions options;
    CURL* easy = nullptr;
    bool have_size = false;
    std::uint64_t size = 0;
    std::string validator;  // ETag or Last-Modified value
    bool validator_is_etag = false;
    struct curl_slist* extra_headers = nullptr;

    ~Impl() {
        if (extra_headers != nullptr)
            curl_slist_free_all(extra_headers);
        if (easy != nullptr)
            curl_easy_cleanup(easy);
    }

    void apply_common() {
        curl_easy_setopt(easy, CURLOPT_URL, url.c_str());
        curl_easy_setopt(easy, CURLOPT_TIMEOUT_MS, options.timeout_ms);
        curl_easy_setopt(easy, CURLOPT_CONNECTTIMEOUT_MS, options.connect_timeout_ms);
        curl_easy_setopt(easy, CURLOPT_FOLLOWLOCATION, options.follow_redirects ? 1L : 0L);
        curl_easy_setopt(easy, CURLOPT_USERAGENT, options.user_agent.c_str());
        // A compressed representation makes byte ranges meaningless.
        curl_easy_setopt(easy, CURLOPT_ACCEPT_ENCODING, "identity");
        curl_easy_setopt(easy, CURLOPT_NOSIGNAL, 1L);
    }

    void rebuild_headers() {
        if (extra_headers != nullptr) {
            curl_slist_free_all(extra_headers);
            extra_headers = nullptr;
        }
        if (options.require_stable_representation && !validator.empty()) {
            const std::string h = validator_is_etag ? ("If-Match: " + validator)
                                                    : ("If-Unmodified-Since: " + validator);
            extra_headers = curl_slist_append(extra_headers, h.c_str());
        }
        curl_easy_setopt(easy, CURLOPT_HTTPHEADER, extra_headers);
    }

    void capture_validator(const std::vector<std::string>& headers) {
        if (!validator.empty())
            return;
        const std::string etag = find_header(headers, "ETag");
        if (!etag.empty()) {
            validator = etag;
            validator_is_etag = true;
            return;
        }
        const std::string lm = find_header(headers, "Last-Modified");
        if (!lm.empty()) {
            validator = lm;
            validator_is_etag = false;
        }
    }
};

CurlRangeReader::CurlRangeReader(const std::string& url, CurlOptions options)
    : impl_(std::make_unique<Impl>()) {
    impl_->url = url;
    impl_->options = std::move(options);

    // Reuse one easy handle across every request: connection reuse and
    // keepalive are where the latency win is, and the traversal already
    // coalesces ranges, so curl_multi would add concurrency the access
    // pattern cannot exploit.
    impl_->easy = curl_easy_init();
    if (impl_->easy == nullptr) {
        throw Error(ErrorCode::HttpError, "curl_easy_init failed");
    }
    impl_->apply_common();
}

CurlRangeReader::~CurlRangeReader() = default;

std::uint64_t CurlRangeReader::total_size() {
    if (impl_->have_size)
        return impl_->size;

    std::vector<std::string> headers;
    std::vector<std::uint8_t> body;

    curl_easy_reset(impl_->easy);
    impl_->apply_common();
    curl_easy_setopt(impl_->easy, CURLOPT_NOBODY, 1L);
    curl_easy_setopt(impl_->easy, CURLOPT_HEADERFUNCTION, header_cb);
    curl_easy_setopt(impl_->easy, CURLOPT_HEADERDATA, &headers);
    curl_easy_setopt(impl_->easy, CURLOPT_WRITEFUNCTION, write_cb);
    curl_easy_setopt(impl_->easy, CURLOPT_WRITEDATA, &body);

    ++request_count_;
    const CURLcode rc = curl_easy_perform(impl_->easy);
    if (rc != CURLE_OK) {
        throw Error(ErrorCode::HttpError, std::string("HEAD failed: ") + curl_easy_strerror(rc));
    }

    long status = 0;
    curl_easy_getinfo(impl_->easy, CURLINFO_RESPONSE_CODE, &status);
    if (status < 200 || status >= 300) {
        throw Error(ErrorCode::HttpError, "HEAD returned status " + std::to_string(status));
    }

    curl_off_t len = -1;
    curl_easy_getinfo(impl_->easy, CURLINFO_CONTENT_LENGTH_DOWNLOAD_T, &len);
    if (len < 0) {
        // Some servers omit Content-Length on HEAD. Fall back to a one-byte
        // range and read the total out of Content-Range.
        auto probe = read(0, 1);
        if (!impl_->have_size) {
            throw Error(ErrorCode::HttpError, "server did not report a resource size");
        }
        return impl_->size;
    }

    impl_->capture_validator(headers);
    impl_->size = static_cast<std::uint64_t>(len);
    impl_->have_size = true;
    return impl_->size;
}

std::vector<std::uint8_t> CurlRangeReader::read(std::uint64_t offset, std::uint64_t length) {
    if (length == 0)
        return {};  // contract: never contact the transport

    const std::uint64_t last = detail::range_end(offset, length) - 1;
    const std::string range = std::to_string(offset) + "-" + std::to_string(last);

    std::vector<std::string> headers;
    std::vector<std::uint8_t> body;

    curl_easy_reset(impl_->easy);
    impl_->apply_common();
    impl_->rebuild_headers();
    curl_easy_setopt(impl_->easy, CURLOPT_RANGE, range.c_str());
    curl_easy_setopt(impl_->easy, CURLOPT_HEADERFUNCTION, header_cb);
    curl_easy_setopt(impl_->easy, CURLOPT_HEADERDATA, &headers);
    curl_easy_setopt(impl_->easy, CURLOPT_WRITEFUNCTION, write_cb);
    curl_easy_setopt(impl_->easy, CURLOPT_WRITEDATA, &body);

    ++request_count_;
    const CURLcode rc = curl_easy_perform(impl_->easy);
    if (rc != CURLE_OK) {
        throw Error(ErrorCode::HttpError,
                    std::string("range request failed: ") + curl_easy_strerror(rc));
    }

    long status = 0;
    curl_easy_getinfo(impl_->easy, CURLINFO_RESPONSE_CODE, &status);
    impl_->capture_validator(headers);

    if (status == 412) {
        throw Error(ErrorCode::HttpError, "resource changed between requests (If-Match failed); "
                                          "the URL is not stable");
    }

    if (status == 416) {
        // Unsatisfiable. Legitimate only when reading at or past the end.
        if (impl_->have_size && offset >= impl_->size)
            return {};
        throw Error(ErrorCode::HttpError, "server returned 416 for an in-range request");
    }

    if (status == 206) {
        // A server may legally answer with a DIFFERENT range than asked for,
        // so never assume the body corresponds to the request.
        const std::string cr = find_header(headers, "Content-Range");
        std::uint64_t s = 0, e = 0, total = 0;
        if (!parse_content_range(cr, s, e, total)) {
            throw Error(ErrorCode::HttpError, "malformed or missing Content-Range on 206");
        }
        if (s != offset) {
            throw Error(ErrorCode::HttpError, "server returned range starting at " +
                                                  std::to_string(s) + ", expected " +
                                                  std::to_string(offset));
        }
        if (body.size() != (e - s + 1)) {
            throw Error(ErrorCode::HttpError, "206 body length disagrees with Content-Range");
        }
        if (!impl_->have_size) {
            impl_->size = total;
            impl_->have_size = true;
        }
        // Short only where the range crossed EOF; anything else is truncation.
        if (body.size() < length && (offset + body.size()) < impl_->size) {
            throw Error(ErrorCode::HttpError, "truncated 206 response");
        }
        return body;
    }

    if (status == 200) {
        // The server ignored Range and sent the whole representation. Do NOT
        // truncate to `length` -- that returns bytes [0, length), not
        // [offset, offset+length). Slice properly instead.
        if (!impl_->have_size) {
            impl_->size = body.size();
            impl_->have_size = true;
        }
        if (offset >= body.size())
            return {};
        const std::uint64_t avail = body.size() - offset;
        const std::uint64_t n = std::min<std::uint64_t>(length, avail);
        return std::vector<std::uint8_t>(body.begin() + static_cast<std::ptrdiff_t>(offset),
                                         body.begin() + static_cast<std::ptrdiff_t>(offset + n));
    }

    throw Error(ErrorCode::HttpError, "unexpected HTTP status " + std::to_string(status));
}

}  // namespace fcb

#endif  // FCB_WITH_CURL
