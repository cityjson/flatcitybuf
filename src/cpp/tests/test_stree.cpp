#include <doctest/doctest.h>

#include <fcb/attribute.hpp>
#include <fcb/reader.hpp>
#include <fcb/stree.hpp>

#include <fcb/generated/header_generated.h>

#include <map>
#include <set>
#include <string>
#include <vector>

using namespace fcb;

static const char* kFixture = FCB_TEST_DATA_DIR "/delft.fcb";

TEST_CASE("stree node count uses branching_factor and breaks at n < bf") {
    // Unlike the R-tree (which breaks at n == 1), the B+tree loop stops when
    // a level fits in one node's worth of separators. stree.rs:462-497.
    CHECK(stree_num_nodes(100, 16) == 107);   // 100 -> 7 (107), 7 < 16
    CHECK(stree_num_nodes(16, 16) == 17);     // 16 -> 1 (17), 1 < 16
    CHECK(stree_num_nodes(10, 16) == 11);
    CHECK(stree_num_nodes(1000, 16) == 1067); // 1000 -> 63 (1063) -> 4 (1067)
    CHECK_THROWS_AS(stree_num_nodes(10, 1), Error);
}

TEST_CASE("payload tag is the MSB, mask is the low 63 bits") {
    CHECK(kPayloadTag == 0x8000000000000000ULL);
    CHECK(kPayloadMask == 0x7FFFFFFFFFFFFFFFULL);
    CHECK(is_payload_ref(kPayloadTag | 1234ULL));
    CHECK_FALSE(is_payload_ref(1234ULL));
    CHECK(payload_offset(kPayloadTag | 1234ULL) == 1234ULL);
}

TEST_CASE("payload entries decode as u32 count then count x u64, all LE") {
    std::vector<std::uint8_t> raw = {
        0x02, 0x00, 0x00, 0x00,
        0x0A, 0, 0, 0, 0, 0, 0, 0,
        0x14, 0, 0, 0, 0, 0, 0, 0,
    };
    auto offsets = decode_payload_entry(bytes_view(raw));
    REQUIRE(offsets.size() == 2);
    CHECK(offsets[0] == 10U);
    CHECK(offsets[1] == 20U);
}

TEST_CASE("a truncated payload entry throws") {
    std::vector<std::uint8_t> raw = {0x05, 0, 0, 0, 1, 2, 3};
    CHECK_THROWS_AS(decode_payload_entry(bytes_view(raw)), Error);
}

/// Collect the value of `field` for every feature, by decoding attributes
/// with the per-object schema. Used as an independent oracle: the index
/// says which features match; this says what the data actually holds.
static std::map<std::string, std::string> string_values(FcbReader& r,
                                                        const std::string& field) {
    std::map<std::string, std::string> out;
    FeatureIterator it = r.select_all();
    while (it.next()) {
        const Feature& f = it.current();
        for (std::size_t i = 0; i < f.city_object_count(); ++i) {
            auto blob = f.object_attributes(i);
            if (blob.empty()) continue;
            auto own = f.object_columns(i);
            for (auto& [name, v] :
                 decode_attributes(blob, own.empty() ? r.header().info().columns : own)) {
                if (name == field && v.type == AttrValue::Type::String) {
                    out[f.id()] = v.s;
                }
            }
        }
    }
    return out;
}

TEST_CASE("Eq on a duplicated string column returns exactly the matching features") {
    // 'status' has 5 unique values across 1115 features, so equal keys are
    // collapsed into payload entries -- this exercises payload resolution.
    FcbReader r = FcbReader::open_file(kFixture);
    auto truth = string_values(r, "status");
    REQUIRE_FALSE(truth.empty());

    // Pick the most common value so the payload path is definitely used.
    std::map<std::string, int> freq;
    for (auto& [id, v] : truth) ++freq[v];
    std::string want;
    int best = 0;
    for (auto& [v, n] : freq) {
        if (n > best) { best = n; want = v; }
    }
    REQUIRE(best > 1);

    std::set<std::string> expected;
    for (auto& [id, v] : truth) {
        if (v == want) expected.insert(id);
    }

    AttrQuery q = {{"status", Operator::Eq, KeyValue::from_string(KeyKind::String50, want)}};
    FeatureIterator it = r.select_attr(q);
    std::set<std::string> got;
    while (it.next()) got.insert(it.current().id());

    CHECK(got == expected);
    CHECK(got.size() == static_cast<std::size_t>(best));
}

TEST_CASE("Eq on a unique string column returns exactly one feature") {
    FcbReader r = FcbReader::open_file(kFixture);
    auto truth = string_values(r, "identificatie");
    REQUIRE_FALSE(truth.empty());

    const std::string want = truth.begin()->second;
    AttrQuery q = {
        {"identificatie", Operator::Eq, KeyValue::from_string(KeyKind::String50, want)}};

    FeatureIterator it = r.select_attr(q);
    std::vector<std::string> got;
    while (it.next()) got.push_back(it.current().id());

    REQUIRE(got.size() == 1);
    CHECK(got[0] == truth.begin()->first);
}

TEST_CASE("Ge, Gt and Eq partition a numeric column consistently") {
    FcbReader r = FcbReader::open_file(kFixture);

    // b3_bouwlagen is a ULong (ColumnType 8) with only 6 distinct values, so Eq is non-empty
    // and Ge/Gt genuinely differ.
    auto count = [&](Operator op, std::uint64_t v) {
        AttrQuery q = {{"b3_bouwlagen", op, KeyValue::from_u64(v)}};
        FeatureIterator it = r.select_attr(q);
        std::set<std::string> ids;
        while (it.next()) ids.insert(it.current().id());
        return ids;
    };

    auto ge = count(Operator::Ge, 2);
    auto gt = count(Operator::Gt, 2);
    auto eq = count(Operator::Eq, 2);

    CHECK_FALSE(ge.empty());
    // Ge is exactly Gt plus Eq, and they are disjoint.
    CHECK(ge.size() == gt.size() + eq.size());
    for (const auto& id : gt) CHECK(ge.count(id) == 1);
    for (const auto& id : eq) CHECK(ge.count(id) == 1);
    for (const auto& id : eq) CHECK(gt.count(id) == 0);
}

TEST_CASE("Le and Lt partition consistently, and Ne is the complement of Eq") {
    FcbReader r = FcbReader::open_file(kFixture);

    auto ids = [&](Operator op, std::uint64_t v) {
        AttrQuery q = {{"b3_bouwlagen", op, KeyValue::from_u64(v)}};
        FeatureIterator it = r.select_attr(q);
        std::set<std::string> s;
        while (it.next()) s.insert(it.current().id());
        return s;
    };

    auto le = ids(Operator::Le, 3);
    auto lt = ids(Operator::Lt, 3);
    auto eq = ids(Operator::Eq, 3);
    CHECK(le.size() == lt.size() + eq.size());
    for (const auto& id : eq) CHECK(lt.count(id) == 0);

    auto ne = ids(Operator::Ne, 3);
    for (const auto& id : eq) CHECK(ne.count(id) == 0);
}

TEST_CASE("multiple conditions are ANDed and strictly narrow the result") {
    FcbReader r = FcbReader::open_file(kFixture);

    auto ids = [&](const AttrQuery& q) {
        FeatureIterator it = r.select_attr(q);
        std::set<std::string> s;
        while (it.next()) s.insert(it.current().id());
        return s;
    };

    AttrQuery one = {{"b3_bouwlagen", Operator::Ge, KeyValue::from_u64(1)}};
    AttrQuery two = {{"b3_bouwlagen", Operator::Ge, KeyValue::from_u64(1)},
                     {"b3_bouwlagen", Operator::Le, KeyValue::from_u64(2)}};

    auto a = ids(one);
    auto b = ids(two);
    CHECK_FALSE(a.empty());
    CHECK_FALSE(b.empty());
    // A test asserting only b.size() <= a.size() would pass even if the
    // second condition were ignored entirely; require a strict reduction.
    CHECK(b.size() < a.size());
    for (const auto& id : b) CHECK(a.count(id) == 1);
}

TEST_CASE("results contain no duplicate features") {
    FcbReader r = FcbReader::open_file(kFixture);
    AttrQuery q = {{"b3_bouwlagen", Operator::Ge, KeyValue::from_u64(1)}};

    FeatureIterator it = r.select_attr(q);
    std::vector<std::string> ids;
    while (it.next()) ids.push_back(it.current().id());

    std::set<std::string> uniq(ids.begin(), ids.end());
    CHECK(ids.size() == uniq.size());
}

TEST_CASE("querying an unindexed or unknown column throws") {
    FcbReader r = FcbReader::open_file(kFixture);
    AttrQuery q = {{"definitely_not_a_column", Operator::Eq, KeyValue::from_u64(1)}};
    CHECK_THROWS_AS(r.select_attr(q), Error);

    AttrQuery empty;
    CHECK_THROWS_AS(r.select_attr(empty), Error);
}

TEST_CASE("exact_index_only returns a superset of the verified result") {
    // For string columns the index yields candidates; verification can only
    // remove, never add.
    FcbReader r = FcbReader::open_file(kFixture);
    auto truth = string_values(r, "identificatie");
    const std::string want = truth.begin()->second;

    AttrQuery q = {
        {"identificatie", Operator::Eq, KeyValue::from_string(KeyKind::String50, want)}};

    std::set<std::string> verified, raw;
    {
        FeatureIterator it = r.select_attr(q);
        while (it.next()) verified.insert(it.current().id());
    }
    {
        AttrQueryOptions o;
        o.exact_index_only = true;
        FeatureIterator it = r.select_attr(q, o);
        while (it.next()) raw.insert(it.current().id());
    }
    CHECK(verified.size() <= raw.size());
    for (const auto& id : verified) CHECK(raw.count(id) == 1);
}
