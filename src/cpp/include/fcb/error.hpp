#pragma once

#include <stdexcept>
#include <string>

namespace fcb {

/// Error categories, mirroring fcb_core::error::Error
/// (src/rust/fcb_core/src/error.rs) so the two implementations report the
/// same failures under the same names.
enum class ErrorCode {
    MissingMagicBytes,
    IllegalHeaderSize,
    InvalidFlatbuffer,
    NoIndex,
    AttributeIndexNotFound,
    NoColumnsInHeader,
    MissingRequiredField,
    UnsupportedColumnType,
    InvalidAttributeValue,
    QueryExecutionError,
    IoError,
    HttpError,
    JsonError,
};

/// Every failure the library reports is one of these. Derives from
/// std::runtime_error so callers that only care that something went wrong
/// can catch it without knowing about fcb at all.
class Error : public std::runtime_error {
  public:
    Error(ErrorCode code, const std::string& message) : std::runtime_error(message), code_(code) {}

    ErrorCode code() const noexcept { return code_; }

  private:
    ErrorCode code_;
};

}  // namespace fcb
