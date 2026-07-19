#include <doctest/doctest.h>

#include <fcb/layout.hpp>

#include <cstdint>
#include <vector>

using namespace fcb;

TEST_CASE("magic bytes validation mirrors Rust check_magic_bytes") {
    // lib.rs:56-58 compares only [0,3) and [4,7), and requires b[3] <= VERSION.
    // Byte 7 is written as 0 but never validated.
    std::vector<std::uint8_t> ok = {'f', 'c', 'b', 1, 'f', 'c', 'b', 0};
    CHECK(check_magic_bytes(bytes_view(ok)));

    std::vector<std::uint8_t> byte7_garbage = {'f', 'c', 'b', 1, 'f', 'c', 'b', 0xAB};
    CHECK(check_magic_bytes(bytes_view(byte7_garbage)));

    std::vector<std::uint8_t> version_zero = {'f', 'c', 'b', 0, 'f', 'c', 'b', 0};
    CHECK(check_magic_bytes(bytes_view(version_zero)));

    std::vector<std::uint8_t> future_version = {'f', 'c', 'b', 2, 'f', 'c', 'b', 0};
    CHECK_FALSE(check_magic_bytes(bytes_view(future_version)));

    std::vector<std::uint8_t> bad_prefix = {'x', 'c', 'b', 1, 'f', 'c', 'b', 0};
    CHECK_FALSE(check_magic_bytes(bytes_view(bad_prefix)));

    std::vector<std::uint8_t> bad_second = {'f', 'c', 'b', 1, 'f', 'c', 'x', 0};
    CHECK_FALSE(check_magic_bytes(bytes_view(bad_second)));

    std::vector<std::uint8_t> too_short = {'f', 'c', 'b', 1};
    CHECK_FALSE(check_magic_bytes(bytes_view(too_short)));
}

TEST_CASE("rtree_index_size matches the Rust formula") {
    // packed_rtree/mod.rs:879-898
    // n=1:  num_nodes=1; n=ceil(1/16)=1, num_nodes=2, n==1 -> break. 2*40
    CHECK(rtree_index_size(1, 16) == 80);
    // n=16: num_nodes=16; n=1, num_nodes=17, break. 17*40
    CHECK(rtree_index_size(16, 16) == 680);
    // n=17: num_nodes=17; n=2, num_nodes=19; n=1, num_nodes=20, break. 20*40
    CHECK(rtree_index_size(17, 16) == 800);
    // n=257: 257 -> 17 (274) -> 2 (276) -> 1 (277). 277*40
    CHECK(rtree_index_size(257, 16) == 11080);
}

TEST_CASE("a node_size below 2 is a corrupt file, not something to clamp") {
    // Rust asserts node_size >= 2 before clamping (packed_rtree/mod.rs:879),
    // so clamping here would invent behaviour Rust never exhibits.
    CHECK_THROWS_AS(rtree_index_size(4, 0), Error);
    CHECK_THROWS_AS(rtree_index_size(4, 1), Error);
    CHECK_NOTHROW(rtree_index_size(4, 2));
}

TEST_CASE("size arithmetic is checked against overflow on hostile input") {
    // features_count is an untrusted u64 straight from the file. num_nodes
    // grows past it and is then multiplied by 40; both must be checked.
    CHECK_THROWS_AS(rtree_index_size(UINT64_MAX, 2), Error);
    CHECK_THROWS_AS(compute_layout(100, UINT64_MAX, 16, 0), Error);
}

TEST_CASE("compute_layout stacks sections with no padding") {
    // header_size 100 -> header_len = 8 + 4 + 100 = 112
    FileLayout l = compute_layout(/*header_size=*/100, /*features_count=*/17,
                                  /*index_node_size=*/16, /*attr_index_size=*/500);
    CHECK(l.header_len == 112);
    CHECK(l.rtree_begin == 112);
    CHECK(l.rtree_size == 800);
    CHECK(l.attr_index_begin == 912);
    CHECK(l.attr_index_size == 500);
    CHECK(l.feature_begin == 1412);
}

TEST_CASE("compute_layout suppresses the rtree when it is absent") {
    FileLayout no_index = compute_layout(100, 17, /*index_node_size=*/0, 0);
    CHECK(no_index.rtree_size == 0);
    CHECK(no_index.feature_begin == 112);

    FileLayout no_features = compute_layout(100, /*features_count=*/0, 16, 0);
    CHECK(no_features.rtree_size == 0);
    CHECK(no_features.feature_begin == 112);
}

TEST_CASE("compute_layout rejects illegal header sizes") {
    CHECK_THROWS_AS(compute_layout(7, 1, 16, 0), Error);
    CHECK_THROWS_AS(compute_layout(536870913, 1, 16, 0), Error);
    CHECK_NOTHROW(compute_layout(8, 1, 16, 0));
    CHECK_NOTHROW(compute_layout(536870912, 1, 16, 0));
}

TEST_CASE("compute_layout rejects an attribute index size that overflows") {
    CHECK_THROWS_AS(compute_layout(100, 1, 16, UINT64_MAX), Error);
}

TEST_CASE("validate_layout_against_size catches sections past end of file") {
    FileLayout l = compute_layout(100, 17, 16, 500);
    CHECK_THROWS_AS(validate_layout_against_size(l, 1000), Error);
    CHECK_NOTHROW(validate_layout_against_size(l, 1412));
    CHECK_NOTHROW(validate_layout_against_size(l, 999999));
}
