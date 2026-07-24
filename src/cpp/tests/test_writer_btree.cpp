#include <fcb/reader.hpp>
#include <fcb/writer/btree_builder.hpp>

#include <algorithm>
#include <cstring>

#include <doctest/doctest.h>

using namespace fcb;

namespace {

/// A RangeReader over an in-memory buffer, so `stree_query` (the existing,
/// already-conformant read side) can be used to verify this milestone's
/// output without writing a temp file. Test-only: production code always
/// reads through `FileRangeReader`/`BufferedRangeReader`.
class MemoryRangeReader : public RangeReader {
  public:
    explicit MemoryRangeReader(std::vector<std::uint8_t> data) : data_(std::move(data)) {}

    std::uint64_t total_size() override { return data_.size(); }

    std::vector<std::uint8_t> read(std::uint64_t offset, std::uint64_t length) override {
        if (offset >= data_.size())
            return {};
        const std::uint64_t end = std::min<std::uint64_t>(offset + length, data_.size());
        return std::vector<std::uint8_t>(data_.begin() + static_cast<std::ptrdiff_t>(offset),
                                         data_.begin() + static_cast<std::ptrdiff_t>(end));
    }

  private:
    std::vector<std::uint8_t> data_;
};

std::vector<std::uint64_t> eq_offsets(const std::vector<std::uint8_t>& bytes, KeyKind kind,
                                      std::uint16_t branching_factor,
                                      std::uint32_t num_unique_items, const KeyValue& key) {
    MemoryRangeReader reader(bytes);
    AttrIndexInfo info{};
    info.column_index = 0;
    info.length = static_cast<std::uint32_t>(bytes.size());
    info.branching_factor = branching_factor;
    info.num_unique_items = num_unique_items;
    info.begin = 0;
    auto results = stree_query(reader, info, kind, Operator::Eq, key);
    std::vector<std::uint64_t> offsets;
    for (const auto& r : results)
        offsets.push_back(r.offset);
    std::sort(offsets.begin(), offsets.end());
    return offsets;
}

}  // namespace

TEST_CASE("build_static_btree throws for branching_factor < 2") {
    std::vector<BtreeEntry> entries{{KeyValue::from_i32(1), 0}};
    CHECK_THROWS_AS(build_static_btree(entries, KeyKind::Int32, 1), Error);
    CHECK_THROWS_AS(build_static_btree(entries, KeyKind::Int32, 0), Error);
}

TEST_CASE("build_static_btree throws for an empty entry list") {
    std::vector<BtreeEntry> entries;
    CHECK_THROWS_AS(build_static_btree(entries, KeyKind::Int32, 4), Error);
}

TEST_CASE("build_static_btree round-trips a single entry through the reader") {
    std::vector<BtreeEntry> entries{{KeyValue::from_i32(42), 1000}};
    BuiltBtreeIndex idx = build_static_btree(entries, KeyKind::Int32, 4);
    CHECK(idx.branching_factor == 4);
    CHECK(idx.num_unique_items == 1);

    auto found = eq_offsets(idx.bytes, KeyKind::Int32, idx.branching_factor, idx.num_unique_items,
                            KeyValue::from_i32(42));
    REQUIRE(found.size() == 1);
    CHECK(found[0] == 1000);

    CHECK(eq_offsets(idx.bytes, KeyKind::Int32, idx.branching_factor, idx.num_unique_items,
                     KeyValue::from_i32(0))
              .empty());
}

// Ported from Rust's own `tree_19items_roundtrip_find_exact`
// (stree.rs:1925-1975): 19 distinct keys 0..18, branching_factor 4,
// offset[i] = i * 100. Rather than hand-verifying the exact node array
// (already done once, by hand, for a smaller branching_factor=3 case, and
// independently confirmed via Fable consultation on this milestone), this
// round-trips the built bytes through the EXISTING, already-conformant
// read-side `stree_query` -- exercising this milestone's output the same
// way M5's oracle tests exercise the R-tree builder's, and incidentally
// covering the exact edge cases Rust's own test suite added this fixture
// to catch (key 18 sits on a level-1 separator boundary).
TEST_CASE("build_static_btree matches Rust's tree_19items_roundtrip_find_exact fixture") {
    std::vector<BtreeEntry> entries;
    for (int i = 0; i <= 18; ++i)
        entries.push_back(BtreeEntry{KeyValue::from_i64(i), static_cast<std::uint64_t>(i) * 100});

    BuiltBtreeIndex idx = build_static_btree(entries, KeyKind::Int64, 4);
    CHECK(idx.num_unique_items == 19);

    for (int i = 0; i <= 18; ++i) {
        auto found = eq_offsets(idx.bytes, KeyKind::Int64, idx.branching_factor,
                                idx.num_unique_items, KeyValue::from_i64(i));
        REQUIRE_MESSAGE(found.size() == 1, "key " << i);
        CHECK(found[0] == static_cast<std::uint64_t>(i) * 100);
    }

    // Not present: one past the max real key (also the sentinel value for
    // any level that runs out of real separators) and a negative key.
    CHECK(eq_offsets(idx.bytes, KeyKind::Int64, idx.branching_factor, idx.num_unique_items,
                     KeyValue::from_i64(19))
              .empty());
    CHECK(eq_offsets(idx.bytes, KeyKind::Int64, idx.branching_factor, idx.num_unique_items,
                     KeyValue::from_i64(-1))
              .empty());
}

// Regression ported from Rust's `test_find_exact_on_max_valued_key`
// (stree.rs:2109-2125): `bool`'s max value IS `true`, so a max-valued
// sentinel separator and a real `true`-keyed leaf are numerically
// indistinguishable -- a bug here manifests as a crash (an inverted slice
// from an off-the-end child index), not a wrong answer.
TEST_CASE("build_static_btree does not crash when the key type's own max value is a real key") {
    std::vector<BtreeEntry> entries;
    for (int i = 0; i < 8; ++i)
        entries.push_back(
            BtreeEntry{KeyValue::from_bool(i % 2 == 0), static_cast<std::uint64_t>(i) * 10});

    BuiltBtreeIndex idx = build_static_btree(entries, KeyKind::Bool, 4);
    // Only 2 unique keys (true, false) despite 8 entries -- every "true" and
    // every "false" collapses into one payload-tagged leaf each.
    CHECK(idx.num_unique_items == 2);

    auto true_hits = eq_offsets(idx.bytes, KeyKind::Bool, idx.branching_factor,
                                idx.num_unique_items, KeyValue::from_bool(true));
    auto false_hits = eq_offsets(idx.bytes, KeyKind::Bool, idx.branching_factor,
                                 idx.num_unique_items, KeyValue::from_bool(false));
    CHECK_FALSE(true_hits.empty());
    CHECK_FALSE(false_hits.empty());
    CHECK(true_hits.size() == 4);
    CHECK(false_hits.size() == 4);
}

TEST_CASE("build_static_btree collapses duplicate keys into one payload entry") {
    std::vector<BtreeEntry> entries{
        {KeyValue::from_string(KeyKind::String50, "same"), 0},
        {KeyValue::from_string(KeyKind::String50, "same"), 100},
        {KeyValue::from_string(KeyKind::String50, "same"), 200},
        {KeyValue::from_string(KeyKind::String50, "other"), 300},
    };
    BuiltBtreeIndex idx = build_static_btree(entries, KeyKind::String50, 4);
    CHECK(idx.num_unique_items == 2);  // "same" and "other", not 4

    auto same_hits =
        eq_offsets(idx.bytes, KeyKind::String50, idx.branching_factor, idx.num_unique_items,
                   KeyValue::from_string(KeyKind::String50, "same"));
    REQUIRE(same_hits.size() == 3);
    CHECK(same_hits[0] == 0);
    CHECK(same_hits[1] == 100);
    CHECK(same_hits[2] == 200);

    auto other_hits =
        eq_offsets(idx.bytes, KeyKind::String50, idx.branching_factor, idx.num_unique_items,
                   KeyValue::from_string(KeyKind::String50, "other"));
    REQUIRE(other_hits.size() == 1);
    CHECK(other_hits[0] == 300);
}

TEST_CASE("build_static_btree handles a leaf count that is an exact multiple of the branching "
          "factor") {
    // Exercises the level-bounds/skip-logic boundary Fable flagged: a leaf
    // group count with no partial remainder should not spuriously trip
    // `is_right_most_child` for a boundary group, nor skip building a root.
    std::vector<BtreeEntry> entries;
    for (int i = 0; i < 12; ++i)  // 12 = 4 * (branching_factor - 1), an exact multiple
        entries.push_back(BtreeEntry{KeyValue::from_i32(i), static_cast<std::uint64_t>(i)});

    BuiltBtreeIndex idx = build_static_btree(entries, KeyKind::Int32, 4);
    CHECK(idx.num_unique_items == 12);
    for (int i = 0; i < 12; ++i) {
        auto found = eq_offsets(idx.bytes, KeyKind::Int32, idx.branching_factor,
                                idx.num_unique_items, KeyValue::from_i32(i));
        REQUIRE_MESSAGE(found.size() == 1, "key " << i);
        CHECK(found[0] == static_cast<std::uint64_t>(i));
    }
}
