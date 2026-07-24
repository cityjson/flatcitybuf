#include <fcb/error.hpp>
#include <fcb/writer/header_serializer.hpp>

#include <doctest/doctest.h>

using namespace fcb;

static const ::Header*
build_header(::flatbuffers::FlatBufferBuilder& fbb, const nlohmann::ordered_json& cj,
             const HeaderWriterOptions& options, const AttributeSchema& attr_schema,
             const AttributeSchema* semantic_attr_schema = nullptr,
             const std::vector<AttributeIndexInfo>* attribute_indices_info = nullptr) {
    auto off =
        to_fcb_header(fbb, cj, options, attr_schema, semantic_attr_schema, attribute_indices_info);
    fbb.Finish(off);
    return flatbuffers::GetRoot<::Header>(fbb.GetBufferPointer());
}

TEST_CASE("to_transform builds scale and translate from CityJSON's transform member") {
    auto transform = nlohmann::ordered_json::parse(R"({
        "scale": [0.001, 0.001, 0.001],
        "translate": [84000.0, 447000.0, 0.0]
    })");
    ::Transform t = to_transform(transform);
    CHECK(t.scale().x() == doctest::Approx(0.001));
    CHECK(t.scale().y() == doctest::Approx(0.001));
    CHECK(t.scale().z() == doctest::Approx(0.001));
    CHECK(t.translate().x() == doctest::Approx(84000.0));
    CHECK(t.translate().y() == doctest::Approx(447000.0));
    CHECK(t.translate().z() == doctest::Approx(0.0));
}

TEST_CASE("parse_reference_system parses the ordinary three-element OGC form") {
    auto parsed = parse_reference_system("https://www.opengis.net/def/crs/EPSG/0/7415");
    REQUIRE(parsed.has_value());
    CHECK(parsed->authority == "EPSG");
    CHECK(parsed->version == 0);
    CHECK(parsed->code == 7415);
}

TEST_CASE("parse_reference_system accepts the http scheme too") {
    auto parsed = parse_reference_system("http://www.opengis.net/def/crs/EPSG/0/4326");
    REQUIRE(parsed.has_value());
    CHECK(parsed->authority == "EPSG");
    CHECK(parsed->code == 4326);
}

TEST_CASE("parse_reference_system defaults version/code to 0 when segments are missing") {
    auto parsed = parse_reference_system("https://www.opengis.net/def/crs/EPSG");
    REQUIRE(parsed.has_value());
    CHECK(parsed->authority == "EPSG");
    CHECK(parsed->version == 0);
    CHECK(parsed->code == 0);
}

TEST_CASE("parse_reference_system defaults to 0 when a segment fails to parse as a whole i32") {
    // Rust's `.parse::<i32>().ok()` fails on trailing garbage -- "7415x" does
    // NOT parse as 7415, it fails whole, same as Rust's own `str::parse`.
    auto parsed = parse_reference_system("https://www.opengis.net/def/crs/EPSG/0/7415x");
    REQUIRE(parsed.has_value());
    CHECK(parsed->code == 0);
}

TEST_CASE("parse_reference_system accepts a leading '+' sign, matching Rust's str::parse::<i32>") {
    // Found by the M4 codex review: `std::from_chars` rejects a leading '+'
    // for signed integers, but Rust's `FromStr` for `i32` accepts one, so
    // ".../EPSG/0/+7415" must still parse as code 7415, not fall back to 0.
    auto parsed = parse_reference_system("https://www.opengis.net/def/crs/EPSG/0/+7415");
    REQUIRE(parsed.has_value());
    CHECK(parsed->code == 7415);
}

TEST_CASE("parse_reference_system falls back to 0 for a malformed leading sign") {
    auto bare_plus = parse_reference_system("https://www.opengis.net/def/crs/EPSG/0/+");
    REQUIRE(bare_plus.has_value());
    CHECK(bare_plus->code == 0);

    auto double_sign = parse_reference_system("https://www.opengis.net/def/crs/EPSG/0/+-7415");
    REQUIRE(double_sign.has_value());
    CHECK(double_sign->code == 0);
}

TEST_CASE("parse_reference_system returns nullopt for a URL not matching the OGC CRS prefix") {
    CHECK_FALSE(parse_reference_system("https://example.com/not-a-crs").has_value());
    CHECK_FALSE(parse_reference_system("not-a-url").has_value());
}

TEST_CASE("to_reference_system builds the FlatBuffers table") {
    flatbuffers::FlatBufferBuilder fbb;
    auto off = to_reference_system(fbb, ParsedReferenceSystem{"EPSG", 0, 7415});
    fbb.Finish(off);
    const auto* rs = flatbuffers::GetRoot<::ReferenceSystem>(fbb.GetBufferPointer());
    REQUIRE(rs->authority() != nullptr);
    CHECK(rs->authority()->str() == "EPSG");
    CHECK(rs->version() == 0);
    CHECK(rs->code() == 7415);
    CHECK(rs->code_string() == nullptr);
}

TEST_CASE("to_extension builds name/url/version, leaving description empty") {
    flatbuffers::FlatBufferBuilder fbb;
    auto off = to_extension(fbb, "Noise", "https://example.com/noise.ext.json", "1.0");
    fbb.Finish(off);
    const auto* ext = flatbuffers::GetRoot<::Extension>(fbb.GetBufferPointer());
    CHECK(ext->name()->str() == "Noise");
    CHECK(ext->url()->str() == "https://example.com/noise.ext.json");
    CHECK(ext->version()->str() == "1.0");
    CHECK(ext->description() == nullptr);
}

TEST_CASE("to_templates_vertices keeps only entries that filter down to exactly 3 numbers") {
    auto vertices = nlohmann::ordered_json::parse(R"([
        [1.0, 2.0, 3.0],
        [1.0, 2.0, "not-a-number", 3.0],
        "not-an-array",
        [1.0, 2.0],
        [1.0, 2.0, 3.0, 4.0]
    ])");
    flatbuffers::FlatBufferBuilder fbb;
    auto off = to_templates_vertices(fbb, vertices);
    fbb.Finish(off);
    const auto* v =
        flatbuffers::GetRoot<::flatbuffers::Vector<const ::DoubleVertex*>>(fbb.GetBufferPointer());
    REQUIRE(v->size() == 2);
    CHECK(v->Get(0)->x() == doctest::Approx(1.0));
    CHECK(v->Get(0)->z() == doctest::Approx(3.0));
    CHECK(v->Get(1)->x() == doctest::Approx(1.0));
    CHECK(v->Get(1)->z() == doctest::Approx(3.0));
}

TEST_CASE("to_point_of_contact builds every scalar and address field in order") {
    auto poc = nlohmann::ordered_json::parse(R"({
        "contactName": "3D geoinformation group",
        "contactType": "organization",
        "role": "pointOfContact",
        "phone": "+31 15 2786153",
        "emailAddress": "info@example.com",
        "website": "https://3d.bk.tudelft.nl",
        "address": {
            "thoroughfareNumber": "1",
            "thoroughfareName": "Julianalaan",
            "locality": "Delft",
            "postcode": "2628BL",
            "country": "the Netherlands"
        }
    })");
    flatbuffers::FlatBufferBuilder fbb;
    PocOffsets offs = to_point_of_contact(fbb, poc);
    REQUIRE(offs.contact_name.has_value());
    REQUIRE(offs.address_postcode.has_value());

    auto version = fbb.CreateString("2.0");
    HeaderBuilder builder(fbb);
    builder.add_version(version);
    builder.add_poc_contact_name(*offs.contact_name);
    builder.add_poc_contact_type(*offs.contact_type);
    builder.add_poc_role(*offs.role);
    builder.add_poc_phone(*offs.phone);
    builder.add_poc_email(*offs.email);
    builder.add_poc_website(*offs.website);
    builder.add_poc_address_thoroughfare_number(*offs.address_thoroughfare_number);
    builder.add_poc_address_thoroughfare_name(*offs.address_thoroughfare_name);
    builder.add_poc_address_locality(*offs.address_locality);
    builder.add_poc_address_postcode(*offs.address_postcode);
    builder.add_poc_address_country(*offs.address_country);
    fbb.Finish(builder.Finish());

    const auto* h = flatbuffers::GetRoot<::Header>(fbb.GetBufferPointer());
    CHECK(h->poc_contact_name()->str() == "3D geoinformation group");
    CHECK(h->poc_contact_type()->str() == "organization");
    CHECK(h->poc_role()->str() == "pointOfContact");
    CHECK(h->poc_phone()->str() == "+31 15 2786153");
    CHECK(h->poc_email()->str() == "info@example.com");
    CHECK(h->poc_website()->str() == "https://3d.bk.tudelft.nl");
    CHECK(h->poc_address_thoroughfare_number()->str() == "1");
    CHECK(h->poc_address_thoroughfare_name()->str() == "Julianalaan");
    CHECK(h->poc_address_locality()->str() == "Delft");
    CHECK(h->poc_address_postcode()->str() == "2628BL");
    CHECK(h->poc_address_country()->str() == "the Netherlands");
}

TEST_CASE("to_point_of_contact prefers postcode over postalCode when both are present") {
    auto poc = nlohmann::ordered_json::parse(R"({
        "contactName": "x", "emailAddress": "x@example.com",
        "address": {"postcode": "AAA", "postalCode": "BBB"}
    })");
    flatbuffers::FlatBufferBuilder fbb;
    PocOffsets offs = to_point_of_contact(fbb, poc);
    REQUIRE(offs.address_postcode.has_value());
    fbb.Finish(*offs.address_postcode);
    const auto* s = flatbuffers::GetRoot<::flatbuffers::String>(fbb.GetBufferPointer());
    CHECK(s->str() == "AAA");
}

TEST_CASE("to_point_of_contact falls back to postalCode when postcode is absent") {
    auto poc = nlohmann::ordered_json::parse(R"({
        "contactName": "x", "emailAddress": "x@example.com",
        "address": {"postalCode": "BBB"}
    })");
    flatbuffers::FlatBufferBuilder fbb;
    PocOffsets offs = to_point_of_contact(fbb, poc);
    REQUIRE(offs.address_postcode.has_value());
    fbb.Finish(*offs.address_postcode);
    const auto* s = flatbuffers::GetRoot<::flatbuffers::String>(fbb.GetBufferPointer());
    CHECK(s->str() == "BBB");
}

TEST_CASE("to_point_of_contact stringifies a non-string address member instead of skipping it") {
    auto poc = nlohmann::ordered_json::parse(R"({
        "contactName": "x", "emailAddress": "x@example.com",
        "address": {"thoroughfareNumber": 134}
    })");
    flatbuffers::FlatBufferBuilder fbb;
    PocOffsets offs = to_point_of_contact(fbb, poc);
    REQUIRE(offs.address_thoroughfare_number.has_value());
    fbb.Finish(*offs.address_thoroughfare_number);
    const auto* s = flatbuffers::GetRoot<::flatbuffers::String>(fbb.GetBufferPointer());
    CHECK(s->str() == "134");
}

TEST_CASE("to_point_of_contact leaves optional scalar and address fields absent when missing") {
    auto poc = nlohmann::ordered_json::parse(R"({
        "contactName": "x", "emailAddress": "x@example.com"
    })");
    flatbuffers::FlatBufferBuilder fbb;
    PocOffsets offs = to_point_of_contact(fbb, poc);
    CHECK_FALSE(offs.contact_type.has_value());
    CHECK_FALSE(offs.role.has_value());
    CHECK_FALSE(offs.phone.has_value());
    CHECK_FALSE(offs.website.has_value());
    CHECK_FALSE(offs.address_thoroughfare_number.has_value());
    CHECK_FALSE(offs.address_postcode.has_value());
}

TEST_CASE("to_point_of_contact falls back to postalCode when postcode is explicitly null") {
    auto poc = nlohmann::ordered_json::parse(R"({
        "contactName": "x", "emailAddress": "x@example.com",
        "address": {"postcode": null, "postalCode": "BBB"}
    })");
    flatbuffers::FlatBufferBuilder fbb;
    PocOffsets offs = to_point_of_contact(fbb, poc);
    REQUIRE(offs.address_postcode.has_value());
    fbb.Finish(*offs.address_postcode);
    const auto* s = flatbuffers::GetRoot<::flatbuffers::String>(fbb.GetBufferPointer());
    CHECK(s->str() == "BBB");
}

TEST_CASE("to_point_of_contact throws MissingRequiredField when contactName is absent") {
    auto poc = nlohmann::ordered_json::parse(R"({"emailAddress": "x@example.com"})");
    flatbuffers::FlatBufferBuilder fbb;
    CHECK_THROWS_AS(to_point_of_contact(fbb, poc), const Error&);
}

TEST_CASE("to_point_of_contact throws MissingRequiredField when emailAddress is not a string") {
    auto poc = nlohmann::ordered_json::parse(R"({"contactName": "x", "emailAddress": 5})");
    flatbuffers::FlatBufferBuilder fbb;
    CHECK_THROWS_AS(to_point_of_contact(fbb, poc), const Error&);
}

TEST_CASE("to_fcb_header builds a minimal header with no metadata") {
    auto cj = nlohmann::ordered_json::parse(R"({
        "type": "CityJSON",
        "version": "2.0",
        "transform": {"scale": [1.0, 1.0, 1.0], "translate": [0.0, 0.0, 0.0]}
    })");
    HeaderWriterOptions options;
    options.feature_count = 3;
    options.index_node_size = 16;
    AttributeSchema empty_schema;

    flatbuffers::FlatBufferBuilder fbb;
    const ::Header* h = build_header(fbb, cj, options, empty_schema);

    CHECK(h->version()->str() == "2.0");
    CHECK(h->features_count() == 3);
    CHECK(h->index_node_size() == 16);
    REQUIRE(h->transform() != nullptr);
    CHECK(h->transform()->scale().x() == doctest::Approx(1.0));
    CHECK(h->columns() != nullptr);
    CHECK(h->columns()->size() == 0);
    CHECK(h->semantic_columns() == nullptr);
    CHECK(h->reference_system() == nullptr);
    CHECK(h->identifier() == nullptr);
    CHECK(h->poc_contact_name() == nullptr);
    CHECK(h->geographical_extent() == nullptr);
}

TEST_CASE("to_fcb_header carries metadata: reference system, extent, identifier, title, poc") {
    auto cj = nlohmann::ordered_json::parse(R"({
        "type": "CityJSON",
        "version": "2.0",
        "transform": {"scale": [1.0, 1.0, 1.0], "translate": [0.0, 0.0, 0.0]},
        "metadata": {
            "geographicalExtent": [0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            "identifier": "urn:example:1",
            "referenceDate": "2023-01-01",
            "referenceSystem": "https://www.opengis.net/def/crs/EPSG/0/7415",
            "title": "Example dataset",
            "pointOfContact": {
                "contactName": "Jane Doe",
                "emailAddress": "jane@example.com"
            }
        }
    })");
    HeaderWriterOptions options;
    AttributeSchema empty_schema;

    flatbuffers::FlatBufferBuilder fbb;
    const ::Header* h = build_header(fbb, cj, options, empty_schema);

    REQUIRE(h->reference_system() != nullptr);
    CHECK(h->reference_system()->authority()->str() == "EPSG");
    CHECK(h->reference_system()->code() == 7415);
    CHECK(h->identifier()->str() == "urn:example:1");
    CHECK(h->reference_date()->str() == "2023-01-01");
    CHECK(h->title()->str() == "Example dataset");
    REQUIRE(h->geographical_extent() != nullptr);
    CHECK(h->geographical_extent()->max().x() == doctest::Approx(1.0));
    CHECK(h->poc_contact_name()->str() == "Jane Doe");
    CHECK(h->poc_email()->str() == "jane@example.com");
}

TEST_CASE("to_fcb_header prefers HeaderWriterOptions.geographical_extent over metadata's own") {
    auto cj = nlohmann::ordered_json::parse(R"({
        "type": "CityJSON", "version": "2.0",
        "transform": {"scale": [1.0, 1.0, 1.0], "translate": [0.0, 0.0, 0.0]},
        "metadata": {"geographicalExtent": [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]}
    })");
    HeaderWriterOptions options;
    options.geographical_extent = std::array<double, 6>{9.0, 9.0, 9.0, 10.0, 10.0, 10.0};
    AttributeSchema empty_schema;

    flatbuffers::FlatBufferBuilder fbb;
    const ::Header* h = build_header(fbb, cj, options, empty_schema);
    REQUIRE(h->geographical_extent() != nullptr);
    CHECK(h->geographical_extent()->min().x() == doctest::Approx(9.0));
}

TEST_CASE("to_fcb_header uses HeaderWriterOptions.geographical_extent when metadata is absent") {
    auto cj = nlohmann::ordered_json::parse(R"({
        "type": "CityJSON", "version": "2.0",
        "transform": {"scale": [1.0, 1.0, 1.0], "translate": [0.0, 0.0, 0.0]}
    })");
    HeaderWriterOptions options;
    options.geographical_extent = std::array<double, 6>{1.0, 2.0, 3.0, 4.0, 5.0, 6.0};
    AttributeSchema empty_schema;

    flatbuffers::FlatBufferBuilder fbb;
    const ::Header* h = build_header(fbb, cj, options, empty_schema);
    REQUIRE(h->geographical_extent() != nullptr);
    CHECK(h->geographical_extent()->min().x() == doctest::Approx(1.0));
    CHECK(h->geographical_extent()->max().z() == doctest::Approx(6.0));
}

TEST_CASE("to_fcb_header writes an empty (but present) extensions vector for {}") {
    auto cj = nlohmann::ordered_json::parse(R"({
        "type": "CityJSON", "version": "2.0",
        "transform": {"scale": [1.0, 1.0, 1.0], "translate": [0.0, 0.0, 0.0]},
        "extensions": {}
    })");
    HeaderWriterOptions options;
    AttributeSchema empty_schema;

    flatbuffers::FlatBufferBuilder fbb;
    const ::Header* h = build_header(fbb, cj, options, empty_schema);
    REQUIRE(h->extensions() != nullptr);
    CHECK(h->extensions()->size() == 0);
}

TEST_CASE("to_fcb_header wires semantic_columns from the semantic attribute schema") {
    auto cj = nlohmann::ordered_json::parse(R"({
        "type": "CityJSON", "version": "2.0",
        "transform": {"scale": [1.0, 1.0, 1.0], "translate": [0.0, 0.0, 0.0]}
    })");
    HeaderWriterOptions options;
    AttributeSchema empty_schema;
    AttributeSchema semantic_schema;
    semantic_schema["parent"] = {0, ::ColumnType::String};

    flatbuffers::FlatBufferBuilder fbb;
    const ::Header* h = build_header(fbb, cj, options, empty_schema, &semantic_schema);
    REQUIRE(h->semantic_columns() != nullptr);
    REQUIRE(h->semantic_columns()->size() == 1);
    CHECK(h->semantic_columns()->Get(0)->name()->str() == "parent");
}

TEST_CASE("to_fcb_header wires the appearance table from the CityJSON appearance member") {
    auto cj = nlohmann::ordered_json::parse(R"({
        "type": "CityJSON", "version": "2.0",
        "transform": {"scale": [1.0, 1.0, 1.0], "translate": [0.0, 0.0, 0.0]},
        "appearance": {
            "materials": [{"name": "roof", "diffuseColor": [0.9, 0.5, 0.1]}]
        }
    })");
    HeaderWriterOptions options;
    AttributeSchema empty_schema;

    flatbuffers::FlatBufferBuilder fbb;
    const ::Header* h = build_header(fbb, cj, options, empty_schema);
    REQUIRE(h->appearance() != nullptr);
    REQUIRE(h->appearance()->materials() != nullptr);
    REQUIRE(h->appearance()->materials()->size() == 1);
    CHECK(h->appearance()->materials()->Get(0)->name()->str() == "roof");
}

TEST_CASE("to_fcb_header writes extensions from the CityJSON extensions member") {
    auto cj = nlohmann::ordered_json::parse(R"({
        "type": "CityJSON", "version": "2.0",
        "transform": {"scale": [1.0, 1.0, 1.0], "translate": [0.0, 0.0, 0.0]},
        "extensions": {
            "Noise": {"url": "https://example.com/noise.ext.json", "version": "1.0"}
        }
    })");
    HeaderWriterOptions options;
    AttributeSchema empty_schema;

    flatbuffers::FlatBufferBuilder fbb;
    const ::Header* h = build_header(fbb, cj, options, empty_schema);
    REQUIRE(h->extensions() != nullptr);
    REQUIRE(h->extensions()->size() == 1);
    CHECK(h->extensions()->Get(0)->name()->str() == "Noise");
    CHECK(h->extensions()->Get(0)->url()->str() == "https://example.com/noise.ext.json");
    CHECK(h->extensions()->Get(0)->version()->str() == "1.0");
}

TEST_CASE("to_fcb_header writes attribute_index entries when provided") {
    auto cj = nlohmann::ordered_json::parse(R"({
        "type": "CityJSON", "version": "2.0",
        "transform": {"scale": [1.0, 1.0, 1.0], "translate": [0.0, 0.0, 0.0]}
    })");
    HeaderWriterOptions options;
    AttributeSchema empty_schema;
    std::vector<AttributeIndexInfo> indices{{0, 100, 32, 7}};

    flatbuffers::FlatBufferBuilder fbb;
    const ::Header* h = build_header(fbb, cj, options, empty_schema, nullptr, &indices);
    REQUIRE(h->attribute_index() != nullptr);
    REQUIRE(h->attribute_index()->size() == 1);
    CHECK(h->attribute_index()->Get(0)->index() == 0);
    CHECK(h->attribute_index()->Get(0)->length() == 100);
    CHECK(h->attribute_index()->Get(0)->branching_factor() == 32);
    CHECK(h->attribute_index()->Get(0)->num_unique_items() == 7);
}
