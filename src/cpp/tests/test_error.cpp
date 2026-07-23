#include <fcb/error.hpp>

#include <string>

#include <doctest/doctest.h>

TEST_CASE("Error carries a code and a message") {
    fcb::Error e(fcb::ErrorCode::MissingMagicBytes, "bad magic");
    CHECK(e.code() == fcb::ErrorCode::MissingMagicBytes);
    CHECK(std::string(e.what()) == "bad magic");
}

TEST_CASE("Error is throwable as std::runtime_error") {
    bool caught = false;
    try {
        throw fcb::Error(fcb::ErrorCode::IllegalHeaderSize, "too big");
    } catch (const std::runtime_error& e) {
        caught = true;
        CHECK(std::string(e.what()) == "too big");
    }
    CHECK(caught);
}
