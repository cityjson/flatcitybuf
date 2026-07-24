#include <fcb/error.hpp>
#include <fcb/writer/header_serializer.hpp>

#include <charconv>
#include <string_view>

namespace fcb {

namespace {

std::string as_str_or_empty(const nlohmann::ordered_json& obj, const std::string& key) {
    auto it = obj.find(key);
    if (it == obj.end() || !it->is_string())
        return std::string();
    return it->get<std::string>();
}

/// A schema-required `String` member (`PointOfContact.contactName`/
/// `.emailAddress`, both non-`Option` in cjseq2's typed model). Rust's typed
/// deserialization rejects the WHOLE document if either is missing or not a
/// string; this writer takes raw JSON that was never run through that
/// validation, so it enforces the same requirement itself, at the point of
/// use, rather than silently writing an empty string for invalid input.
std::string require_string_field(const nlohmann::ordered_json& obj, const std::string& key) {
    auto it = obj.find(key);
    if (it == obj.end() || !it->is_string())
        throw Error(ErrorCode::MissingRequiredField,
                    "pointOfContact." + key + " is required and must be a string");
    return it->get<std::string>();
}

/// A schema-OPTIONAL `Option<String>` member (identifier/referenceDate/
/// title, and every `PointOfContact` field but the two above). Disclosed,
/// intentional leniency: Rust's typed model would also reject the whole
/// document if one of these were present with the wrong JSON type (e.g. a
/// number where a string is required) -- `Option<String>` accepts absence
/// or `null`, not type mismatch -- but this writer treats "present with the
/// wrong type" the same as "absent" rather than failing the whole header,
/// matching this milestone's general policy of favoring a producible file
/// over replicating Rust's document-level rejection for malformed optional
/// metadata (see also `parse_reference_system`'s prefix-mismatch note).
std::optional<std::string> optional_string_field(const nlohmann::ordered_json& obj,
                                                 const std::string& key) {
    auto it = obj.find(key);
    if (it == obj.end() || !it->is_string())
        return std::nullopt;
    return it->get<std::string>();
}

/// Mirrors the `address_member` closure in `to_point_of_contact`
/// (writer/serializer.rs:343-351): a string member is kept verbatim; `null`
/// is treated as absent; anything else (number, object, array, bool) falls
/// back to its JSON text, matching serde_json::Value's `Display` impl that
/// Rust's `other.to_string()` uses there.
std::optional<std::string> address_member(const nlohmann::ordered_json& address,
                                          const std::string& key) {
    auto it = address.find(key);
    if (it == address.end() || it->is_null())
        return std::nullopt;
    if (it->is_string())
        return it->get<std::string>();
    return it->dump();
}

std::optional<std::string> address_either(const nlohmann::ordered_json& address,
                                          const std::string& a, const std::string& b) {
    if (auto v = address_member(address, a))
        return v;
    return address_member(address, b);
}

std::int32_t parse_i32_whole(std::string_view s) {
    if (s.empty())
        return 0;
    // `std::from_chars` rejects a leading '+' for signed integers, but
    // Rust's `str::parse::<i32>()` accepts one (its `FromStr` impl allows an
    // optional leading `+` or `-`) -- ".../EPSG/0/+7415" is a legal `code`
    // segment there. Strip it here, but only when a digit actually follows,
    // so a malformed "+-7415" or bare "+" still falls through to failure
    // below rather than silently parsing "-7415".
    if (s.front() == '+') {
        s.remove_prefix(1);
        if (s.empty() || !(s.front() >= '0' && s.front() <= '9'))
            return 0;
    }
    std::int32_t value = 0;
    auto [ptr, ec] = std::from_chars(s.data(), s.data() + s.size(), value);
    if (ec != std::errc() || ptr != s.data() + s.size())
        return 0;
    return value;
}

}  // namespace

::Transform to_transform(const nlohmann::ordered_json& transform) {
    const auto& scale = transform.at("scale");
    const auto& translate = transform.at("translate");
    return ::Transform(
        ::Vector(scale.at(0).get<double>(), scale.at(1).get<double>(), scale.at(2).get<double>()),
        ::Vector(translate.at(0).get<double>(), translate.at(1).get<double>(),
                 translate.at(2).get<double>()));
}

std::optional<ParsedReferenceSystem> parse_reference_system(const std::string& url) {
    static constexpr std::string_view kPrefixes[] = {"http://www.opengis.net/def/crs/",
                                                     "https://www.opengis.net/def/crs/"};
    for (const auto& prefix : kPrefixes) {
        if (url.compare(0, prefix.size(), prefix) != 0)
            continue;

        const std::string rest = url.substr(prefix.size());
        std::vector<std::string> segments;
        std::size_t start = 0;
        while (true) {
            const std::size_t pos = rest.find('/', start);
            const std::size_t end = pos == std::string::npos ? rest.size() : pos;
            segments.push_back(rest.substr(start, end - start));
            if (pos == std::string::npos)
                break;
            start = pos + 1;
        }

        ParsedReferenceSystem out;
        out.authority = segments[0];
        out.version = segments.size() > 1 ? parse_i32_whole(segments[1]) : 0;
        out.code = segments.size() > 2 ? parse_i32_whole(segments[2]) : 0;
        return out;
    }
    return std::nullopt;
}

::flatbuffers::Offset<::ReferenceSystem> to_reference_system(::flatbuffers::FlatBufferBuilder& fbb,
                                                             const ParsedReferenceSystem& ref_sys) {
    auto authority = fbb.CreateString(ref_sys.authority);
    return CreateReferenceSystem(fbb, authority, ref_sys.version, ref_sys.code, 0);
}

::flatbuffers::Offset<::Extension> to_extension(::flatbuffers::FlatBufferBuilder& fbb,
                                                const std::string& name, const std::string& url,
                                                const std::string& version) {
    auto name_off = fbb.CreateString(name);
    auto url_off = fbb.CreateString(url);
    auto version_off = fbb.CreateString(version);
    return CreateExtension(fbb, name_off, 0, url_off, version_off);
}

::flatbuffers::Offset<::flatbuffers::Vector<const ::DoubleVertex*>>
to_templates_vertices(::flatbuffers::FlatBufferBuilder& fbb,
                      const nlohmann::ordered_json& vertices_templates) {
    std::vector<::DoubleVertex> verts;
    if (vertices_templates.is_array()) {
        for (const auto& v : vertices_templates) {
            if (!v.is_array())
                continue;
            std::vector<double> coords;
            for (const auto& c : v)
                if (c.is_number())
                    coords.push_back(c.get<double>());
            if (coords.size() == 3)
                verts.emplace_back(coords[0], coords[1], coords[2]);
        }
    }
    return fbb.CreateVectorOfStructs(verts);
}

PocOffsets to_point_of_contact(::flatbuffers::FlatBufferBuilder& fbb,
                               const nlohmann::ordered_json& poc) {
    PocOffsets out;
    out.contact_name = fbb.CreateString(require_string_field(poc, "contactName"));

    if (auto v = optional_string_field(poc, "contactType"))
        out.contact_type = fbb.CreateString(*v);
    if (auto v = optional_string_field(poc, "role"))
        out.role = fbb.CreateString(*v);
    if (auto v = optional_string_field(poc, "phone"))
        out.phone = fbb.CreateString(*v);
    out.email = fbb.CreateString(require_string_field(poc, "emailAddress"));
    if (auto v = optional_string_field(poc, "website"))
        out.website = fbb.CreateString(*v);

    // `address`'s presence check is the same disclosed leniency as
    // `optional_string_field`: Rust's `Option<Address>` would reject the
    // whole document if `address` were present but not a JSON object (an
    // `Address`'s `#[serde(flatten)]` map requires object shape); this
    // writer just treats it as absent instead.
    if (auto addr_it = poc.find("address"); addr_it != poc.end() && addr_it->is_object()) {
        const nlohmann::ordered_json& address = *addr_it;
        if (auto v = address_member(address, "thoroughfareNumber"))
            out.address_thoroughfare_number = fbb.CreateString(*v);
        if (auto v = address_member(address, "thoroughfareName"))
            out.address_thoroughfare_name = fbb.CreateString(*v);
        if (auto v = address_member(address, "locality"))
            out.address_locality = fbb.CreateString(*v);
        if (auto v = address_either(address, "postcode", "postalCode"))
            out.address_postcode = fbb.CreateString(*v);
        if (auto v = address_member(address, "country"))
            out.address_country = fbb.CreateString(*v);
    }
    return out;
}

::flatbuffers::Offset<::Header>
to_fcb_header(::flatbuffers::FlatBufferBuilder& fbb, const nlohmann::ordered_json& cj,
              const HeaderWriterOptions& options, const AttributeSchema& attr_schema,
              const AttributeSchema* semantic_attr_schema,
              const std::vector<AttributeIndexInfo>* attribute_indices_info) {
    auto version = fbb.CreateString(cj.at("version").get<std::string>());
    ::Transform transform = to_transform(cj.at("transform"));
    const std::uint64_t features_count = options.feature_count;

    auto columns = to_columns(fbb, attr_schema);
    std::optional<::flatbuffers::Offset<::flatbuffers::Vector<::flatbuffers::Offset<::Column>>>>
        semantic_columns;
    if (semantic_attr_schema != nullptr)
        semantic_columns = to_columns(fbb, *semantic_attr_schema);

    const std::uint16_t index_node_size = options.index_node_size;

    std::optional<::flatbuffers::Offset<::flatbuffers::Vector<const ::AttributeIndex*>>>
        attribute_index;
    if (attribute_indices_info != nullptr) {
        std::vector<::AttributeIndex> entries;
        entries.reserve(attribute_indices_info->size());
        for (const auto& info : *attribute_indices_info)
            entries.emplace_back(info.index, info.length, info.branching_factor,
                                 info.num_unique_items);
        attribute_index = fbb.CreateVectorOfStructs(entries);
    }

    std::optional<::flatbuffers::Offset<::flatbuffers::Vector<::flatbuffers::Offset<::Extension>>>>
        extensions;
    if (auto it = cj.find("extensions"); it != cj.end() && it->is_object()) {
        std::vector<::flatbuffers::Offset<::Extension>> ext_offs;
        for (const auto& [name, ext] : it->items())
            ext_offs.push_back(to_extension(fbb, name, as_str_or_empty(ext, "url"),
                                            as_str_or_empty(ext, "version")));
        extensions = fbb.CreateVector(ext_offs);
    }

    std::optional<::GeographicalExtent> geographical_extent;
    if (options.geographical_extent)
        geographical_extent = to_geographical_extent(*options.geographical_extent);

    std::optional<::flatbuffers::Offset<::Appearance>> appearance;
    if (auto it = cj.find("appearance"); it != cj.end() && it->is_object())
        appearance = to_appearance(fbb, *it);

    std::optional<::flatbuffers::Offset<::flatbuffers::Vector<const ::DoubleVertex*>>>
        templates_vertices;
    std::optional<::flatbuffers::Offset<::flatbuffers::Vector<::flatbuffers::Offset<::Geometry>>>>
        templates;
    if (auto gm_it = cj.find("geometry-templates"); gm_it != cj.end() && gm_it->is_object()) {
        templates_vertices = to_templates_vertices(fbb, gm_it->at("vertices-templates"));

        std::vector<::flatbuffers::Offset<::Geometry>> geom_offs;
        for (const auto& g : gm_it->at("templates"))
            geom_offs.push_back(to_geometry(fbb, g, semantic_attr_schema));
        templates = fbb.CreateVector(geom_offs);
    }

    std::optional<::flatbuffers::Offset<::ReferenceSystem>> reference_system;
    std::optional<::flatbuffers::Offset<::flatbuffers::String>> identifier;
    std::optional<::flatbuffers::Offset<::flatbuffers::String>> reference_date;
    std::optional<::flatbuffers::Offset<::flatbuffers::String>> title;
    PocOffsets poc;

    if (auto meta_it = cj.find("metadata"); meta_it != cj.end() && meta_it->is_object()) {
        const nlohmann::ordered_json& meta = *meta_it;

        if (auto rs_it = meta.find("referenceSystem"); rs_it != meta.end() && rs_it->is_string()) {
            if (auto parsed = parse_reference_system(rs_it->get<std::string>()))
                reference_system = to_reference_system(fbb, *parsed);
        }

        if (!geographical_extent) {
            if (auto ge_it = meta.find("geographicalExtent");
                ge_it != meta.end() && ge_it->is_array() && ge_it->size() == 6) {
                std::array<double, 6> extent{};
                for (std::size_t i = 0; i < 6; ++i)
                    extent[i] = ge_it->at(i).get<double>();
                geographical_extent = to_geographical_extent(extent);
            }
        }

        if (auto v = optional_string_field(meta, "identifier"))
            identifier = fbb.CreateString(*v);
        if (auto v = optional_string_field(meta, "referenceDate"))
            reference_date = fbb.CreateString(*v);
        if (auto v = optional_string_field(meta, "title"))
            title = fbb.CreateString(*v);

        if (auto poc_it = meta.find("pointOfContact"); poc_it != meta.end() && poc_it->is_object())
            poc = to_point_of_contact(fbb, *poc_it);
    }

    // `CreateHeader` (flatc-generated, header_generated.h) is used here
    // instead of hand-sequenced `HeaderBuilder::add_*` calls: flatc's
    // generated `Create*` helper adds fields in a fixed, WIDTH-sorted order
    // (widest fields first) to minimize padding, and it does so IDENTICALLY
    // across every language backend -- which is the only reason Rust's and
    // C++'s output can be byte-identical at all. A field's byte offset
    // WITHIN the table is determined by the order its `add_*` was actually
    // CALLED (each call appends at the builder's current cursor), so calling
    // them by hand in any other order -- e.g. grouped by CityJSON-source
    // semantics, as this function's own local-variable order reads -- lays
    // the table out differently even though every field ends up present
    // with the right value: a real byte-exact regression this milestone's
    // own oracle test caught (see test_writer_oracle.cpp).
    return CreateHeader(
        fbb, &transform, appearance.value_or(0), columns, semantic_columns.value_or(0),
        features_count, index_node_size, attribute_index.value_or(0),
        geographical_extent ? &*geographical_extent : nullptr, reference_system.value_or(0),
        identifier.value_or(0), reference_date.value_or(0), title.value_or(0),
        templates.value_or(0), templates_vertices.value_or(0), extensions.value_or(0),
        poc.contact_name.value_or(0), poc.contact_type.value_or(0), poc.role.value_or(0),
        poc.phone.value_or(0), poc.email.value_or(0), poc.website.value_or(0),
        poc.address_thoroughfare_number.value_or(0), poc.address_thoroughfare_name.value_or(0),
        poc.address_locality.value_or(0), poc.address_postcode.value_or(0),
        poc.address_country.value_or(0), /*attributes=*/0, version);
}

}  // namespace fcb
