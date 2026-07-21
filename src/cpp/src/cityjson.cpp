#include <fcb/cityjson.hpp>

#ifdef FCB_WITH_JSON

#include <fcb/attribute.hpp>
#include <fcb/geometry.hpp>

#include "detail/feature_access.hpp"
#include "detail/header_access.hpp"

#include <fcb/generated/feature_generated.h>
#include <fcb/generated/header_generated.h>

#include <array>
#include <charconv>
#include <cstring>
#include <string>

namespace fcb {

namespace {

// ---------------------------------------------------------------------------
// UNKNOWN-TAG POLICY
//
// Three enumerators reach this file with no CityJSON name of their own:
// CityObjectType::ExtensionObject, SemanticSurfaceType::ExtraSemanticSurface,
// and any GeometryType a newer encoder might add. One policy per tag, and each
// matches the Rust reader exactly (deserializer.rs::to_cj_co_type,
// geom_decoder.rs::to_cj_surface_type, geom_decoder.rs::GeometryType::to_cj).
//
// A City Object type and a semantic surface type each have a CityJSON
// extension escape hatch (spec sections 8 and 3.3): any name starting with '+'
// is legal. So an unnameable tag is spelled "+UnknownCityObject" /
// "+GenericSurface" -- a placeholder, but a SCHEMA-VALID one. These names
// deliberately do NOT appear in the tables below: "ExtensionObject" and
// "ExtraSemanticSurface" are FlatBuffers enumerator names, not CityJSON type
// names, they carry no '+', and emitting either produces a document an
// official validator rejects. That is as much a defect as rejecting a valid
// one.
//
// A geometry type has no such escape hatch -- CityJSON section 3 enumerates
// exactly eight `type` values and admits no others -- so there is no valid
// string to fall back to and geometry_type_name() throws instead. See
// geometry.cpp.
//
// The appearance enums (wrapMode, textureType) throw as well, on both sides.
// ---------------------------------------------------------------------------

/// Names must match CityJSON exactly; the enum order comes from
/// feature.fbs's CityObjectType declaration. ExtensionObject, the last
/// enumerator, is deliberately absent: see the policy note above.
const char* const kCityObjectTypeNames[] = {
    "Bridge", "BridgePart", "BridgeInstallation", "BridgeConstructiveElement",
    "BridgeRoom", "BridgeFurniture",
    "Building", "BuildingPart", "BuildingInstallation",
    "BuildingConstructiveElement", "BuildingFurniture", "BuildingStorey",
    "BuildingRoom", "BuildingUnit",
    "CityFurniture", "CityObjectGroup", "GenericCityObject", "LandUse",
    "OtherConstruction", "PlantCover", "SolitaryVegetationObject", "TINRelief",
    "Road", "Railway", "Waterway", "TransportSquare",
    "Tunnel", "TunnelPart", "TunnelInstallation", "TunnelConstructiveElement",
    "TunnelHollowSpace", "TunnelFurniture",
    "WaterBody",
};

/// The CityJSON spelling of a City Object tag this reader cannot name.
constexpr const char* kUnknownCityObjectName = "+UnknownCityObject";

UIntView as_uint_view(const flatbuffers::Vector<std::uint32_t>* v) {
    if (v == nullptr) return {};
    return UIntView(v->data(), v->size());
}

const char* const kSemanticSurfaceTypeNames[] = {
    "RoofSurface", "GroundSurface", "WallSurface", "ClosureSurface",
    "OuterCeilingSurface", "OuterFloorSurface", "Window", "Door",
    "InteriorWallSurface", "CeilingSurface", "FloorSurface",
    "WaterSurface", "WaterGroundSurface", "WaterClosureSurface",
    "TrafficArea", "AuxiliaryTrafficArea", "TransportationMarking",
    "TransportationHole",
};

/// The CityJSON spelling of a semantic surface tag this reader cannot name.
constexpr const char* kGenericSurfaceName = "+GenericSurface";

GeometryKind kind_of(const ::Geometry* g) {
    return static_cast<GeometryKind>(static_cast<std::uint8_t>(g->type()));
}

/// A material colour: `appearance.schema.json` fixes `diffuseColor`,
/// `emissiveColor` and `specularColor` at exactly three numbers, which
/// `cjseq::Color` spells as `[f64; 3]`. A stored vector of any other length is
/// not a colour and is dropped rather than padded or truncated
/// (deserializer.rs::to_color). The reader used to emit whatever length was
/// stored, which the Rust reader no longer does.
nlohmann::json color_to_json(const flatbuffers::Vector<double>* c) {
    if (c == nullptr || c->size() != 3) return nullptr;
    auto out = nlohmann::json::array();
    for (double v : *c) out.push_back(v);
    return out;
}

/// A texture's `borderColor`, the one CityJSON colour that is not fixed at
/// three numbers: `"minItems": 3, "maxItems": 4`. Any other length is dropped
/// (deserializer.rs::to_border_color).
nlohmann::json border_color_to_json(const flatbuffers::Vector<double>* c) {
    if (c == nullptr || (c->size() != 3 && c->size() != 4)) return nullptr;
    auto out = nlohmann::json::array();
    for (double v : *c) out.push_back(v);
    return out;
}

/// An unrecognised FlatBuffers enumeration tag is an error, never a default.
///
/// The reference used to fall back to the first table entry, which is exactly
/// how a texture written `"wrapMode": "wrap"` came back as `"none"` -- a bug
/// only the C++ conformance corpus caught. `deserializer.rs` now returns
/// `Error::UnknownEnumTag`; this is its equivalent. A tag we do not know means
/// the file was written by a newer writer or is corrupt, and the caller is
/// told so rather than handed a valid-looking lie.
[[noreturn]] void unknown_enum_tag(const char* member, int tag) {
    throw Error(ErrorCode::InvalidFlatbuffer,
                std::string("unknown ") + member + " tag " + std::to_string(tag));
}

/// CityJSON's three appearance enumerations use three different casings:
/// `type` is UPPER, `wrapMode` and `textureType` are lower. Verified against
/// `appearance.schema.json`, not from memory.
const char* texture_format_name(::TextureFormat f) {
    switch (f) {
        case ::TextureFormat::PNG: return "PNG";
        case ::TextureFormat::JPG: return "JPG";
    }
    unknown_enum_tag("type", static_cast<int>(f));
}

const char* wrap_mode_name(::WrapMode w) {
    switch (w) {
        case ::WrapMode::None: return "none";
        case ::WrapMode::Wrap: return "wrap";
        case ::WrapMode::Mirror: return "mirror";
        case ::WrapMode::Clamp: return "clamp";
        case ::WrapMode::Border: return "border";
    }
    unknown_enum_tag("wrapMode", static_cast<int>(w));
}

const char* texture_type_name(::TextureType t) {
    switch (t) {
        case ::TextureType::Unknown: return "unknown";
        case ::TextureType::Specific: return "specific";
        case ::TextureType::Typical: return "typical";
    }
    unknown_enum_tag("textureType", static_cast<int>(t));
}

/// The `appearance` object a CityJSONFeature carries: the materials and
/// textures its geometry mappings index into, plus the UV vertices those
/// textures address. Mirrors deserializer.rs:502-599.
nlohmann::json appearance_to_json(const ::Appearance* a) {
    nlohmann::json out = nlohmann::json::object();

    if (a->materials() != nullptr) {
        auto materials = nlohmann::json::array();
        for (const auto* m : *a->materials()) {
            if (m == nullptr) continue;
            nlohmann::json j = nlohmann::json::object();
            j["name"] = (m->name() != nullptr) ? m->name()->str() : "";
            // Every other field is optional and omitted when unset, which
            // is what serde's skip_serializing_if does on the Rust side.
            if (const auto v = m->ambient_intensity()) j["ambientIntensity"] = *v;
            if (auto c = color_to_json(m->diffuse_color()); !c.is_null()) j["diffuseColor"] = c;
            if (auto c = color_to_json(m->emissive_color()); !c.is_null()) j["emissiveColor"] = c;
            if (auto c = color_to_json(m->specular_color()); !c.is_null()) j["specularColor"] = c;
            if (const auto v = m->shininess()) j["shininess"] = *v;
            if (const auto v = m->transparency()) j["transparency"] = *v;
            if (const auto v = m->is_smooth()) j["isSmooth"] = *v;
            materials.push_back(std::move(j));
        }
        out["materials"] = std::move(materials);
    }

    if (a->textures() != nullptr) {
        auto textures = nlohmann::json::array();
        for (const auto* t : *a->textures()) {
            if (t == nullptr) continue;
            nlohmann::json j = nlohmann::json::object();
            j["type"] = texture_format_name(t->type());
            // `image` is mandatory in header.fbs but optional in CityJSON, so
            // the writer stores "" both for an absent `image` and for a
            // schema-valid `"image": ""`. The two are indistinguishable on the
            // wire and one of them must be lost; decoding "" back to ABSENT is
            // a deliberate choice mirrored from deserializer.rs, not something
            // derivable from the schema. Emitting `"image": ""` here instead
            // is what this reader used to do, and it diverged.
            if (t->image() != nullptr && t->image()->size() > 0) {
                j["image"] = t->image()->str();
            }
            if (const auto w = t->wrap_mode()) j["wrapMode"] = wrap_mode_name(*w);
            if (const auto tt = t->texture_type()) j["textureType"] = texture_type_name(*tt);
            if (auto c = border_color_to_json(t->border_color()); !c.is_null()) {
                j["borderColor"] = c;
            }
            textures.push_back(std::move(j));
        }
        out["textures"] = std::move(textures);
    }

    // UV pairs. Vec2 is a struct of two doubles; read via memcpy for the
    // same alignment reason as Transform and DoubleVertex.
    if (a->vertices_texture() != nullptr) {
        auto uvs = nlohmann::json::array();
        for (const auto* v : *a->vertices_texture()) {
            double u;
            double w;
            std::memcpy(&u, reinterpret_cast<const std::uint8_t*>(v), sizeof(double));
            std::memcpy(&w, reinterpret_cast<const std::uint8_t*>(v) + sizeof(double),
                        sizeof(double));
            uvs.push_back(nlohmann::json::array({u, w}));
        }
        out["vertices-texture"] = std::move(uvs);
    }

    if (a->default_theme_texture() != nullptr) {
        out["default-theme-texture"] = a->default_theme_texture()->str();
    }
    if (a->default_theme_material() != nullptr) {
        out["default-theme-material"] = a->default_theme_material()->str();
    }

    return out;
}

/// `material`: theme -> either a single shared-material index or a nested
/// values array. The key itself is omitted only when the mapping vector is
/// absent or empty, which the caller checks (geom_decoder.rs:343).
///
/// ABSENT-VS-EMPTY, first of three places. A mapping with NO `vertices`
/// vector at all is `"values": null` -- a member that is present with a null
/// value, which the schema requires and permits. A mapping whose `vertices`
/// is present but empty is `"values": []`. Collapsing the two onto "skip the
/// theme" is what dropped an explicit null.
nlohmann::json materials_to_json(const flatbuffers::Vector<
                                 flatbuffers::Offset<::MaterialMapping>>* mappings,
                                 GeometryKind type) {
    nlohmann::json out = nlohmann::json::object();
    for (const auto* m : *mappings) {
        if (m == nullptr) continue;
        const std::string theme = (m->theme() != nullptr) ? m->theme()->str() : "theme";
        // A `value` colours the whole object and has no depth at all.
        if (const auto v = m->value()) {
            out[theme] = {{"value", *v}};
            continue;
        }
        if (m->vertices() == nullptr) {
            out[theme] = {{"values", nullptr}};
            continue;
        }
        out[theme] = {{"values", decode_material_values(type,
                                                        as_uint_view(m->solids()),
                                                        as_uint_view(m->shells()),
                                                        as_uint_view(m->vertices()))}};
    }
    return out;
}

/// `texture`: theme -> nested values array.
///
/// ABSENT-VS-EMPTY, second of three. The per-theme texture object carries no
/// `required` keyword, so a theme with no `values` member at all is valid
/// CityJSON and is written with no `vertices` vector; it decodes to an empty
/// object, NOT to a missing theme and NOT to `"values": []`. A present but
/// empty `vertices` is `"values": []` (geom_decoder.rs:483).
nlohmann::json textures_to_json(const flatbuffers::Vector<
                                flatbuffers::Offset<::TextureMapping>>* mappings,
                                GeometryKind type) {
    nlohmann::json out = nlohmann::json::object();
    for (const auto* m : *mappings) {
        if (m == nullptr) continue;
        const std::string theme = (m->theme() != nullptr) ? m->theme()->str() : "theme";
        if (m->vertices() == nullptr) {
            out[theme] = nlohmann::json::object();
            continue;
        }
        out[theme] = {{"values", decode_texture_values(type,
                                                       as_uint_view(m->solids()),
                                                       as_uint_view(m->shells()),
                                                       as_uint_view(m->surfaces()),
                                                       as_uint_view(m->strings()),
                                                       as_uint_view(m->vertices()))}};
    }
    return out;
}

nlohmann::json geometry_instance_to_json(const ::GeometryInstance* gi) {
    nlohmann::json out = nlohmann::json::object();
    out["type"] = "GeometryInstance";
    out["template"] = gi->template_();

    // The boundaries array holds exactly one vertex index: CityGML's
    // "referencePoint" for the instance.
    auto b = nlohmann::json::array();
    if (gi->boundaries() != nullptr) {
        for (std::uint32_t v : *gi->boundaries()) b.push_back(v);
    }
    out["boundaries"] = std::move(b);

    // 16 doubles in row-major order. Read via memcpy: like Transform in the
    // header, this struct can sit at a misaligned internal offset.
    if (const auto* m = gi->transformation()) {
        auto mat = nlohmann::json::array();
        for (std::size_t i = 0; i < 16; ++i) {
            double d;
            std::memcpy(&d, reinterpret_cast<const std::uint8_t*>(m) + i * sizeof(double),
                        sizeof(double));
            mat.push_back(d);
        }
        out["transformationMatrix"] = std::move(mat);
    }
    return out;
}

nlohmann::json geometry_to_json(const ::Geometry* g,
                                const std::vector<ColumnInfo>& semantic_columns) {
    nlohmann::json out = nlohmann::json::object();
    out["type"] = geometry_type_name(static_cast<std::uint8_t>(g->type()));
    if (g->lod() != nullptr) out["lod"] = g->lod()->str();

    out["boundaries"] = decode_boundaries(
        kind_of(g), as_uint_view(g->solids()), as_uint_view(g->shells()),
        as_uint_view(g->surfaces()), as_uint_view(g->strings()),
        as_uint_view(g->boundaries()));

    // Semantics: a surface list plus a values array indexing into it.
    // ABSENT-VS-EMPTY, fourth site. The reference keys the `semantics` member
    // off the PRESENCE of `semantics_objects` alone (deserializer.rs:699), so
    // a present-but-EMPTY surface vector is `"surfaces": []` -- a semantics
    // member with no surfaces -- and not a missing member. The writer emits
    // exactly that shape (serializer.rs:946 calls create_vector on a possibly
    // empty Vec), so a `size() > 0` guard here dropped the whole member.
    if (g->semantics_objects() != nullptr) {
        auto surfaces = nlohmann::json::array();
        for (const auto* so : *g->semantics_objects()) {
            if (so == nullptr) continue;
            nlohmann::json s = nlohmann::json::object();
            s["type"] = (so->extension_type() != nullptr)
                            ? so->extension_type()->str()
                            : semantic_surface_type_name(
                                  static_cast<std::uint8_t>(so->type()));
            // `parent:uint = null` in the schema -- absent and zero are both
            // real states, so this must check the Optional rather than
            // testing for non-zero. Links a Door/Window surface back to its
            // WallSurface (geom_decoder.rs:217).
            if (const auto p = so->parent()) s["parent"] = *p;
            // Semantic surfaces carry their own attributes, decoded against
            // Header.semantic_columns -- a schema separate from the feature
            // attribute columns. Merged inline, as the reference does.
            if (so->attributes() != nullptr && so->attributes()->size() > 0) {
                auto attrs = attributes_to_json(
                    bytes_view(so->attributes()->data(), so->attributes()->size()),
                    semantic_columns);
                for (auto& [k, v] : attrs.items()) s[k] = v;
            }
            if (so->children() != nullptr && so->children()->size() > 0) {
                auto kids = nlohmann::json::array();
                for (std::uint32_t c : *so->children()) kids.push_back(c);
                s["children"] = std::move(kids);
            }
            surfaces.push_back(std::move(s));
        }

        nlohmann::json sem = nlohmann::json::object();
        sem["surfaces"] = std::move(surfaces);
        // ABSENT-VS-EMPTY, third of three. The SURFACES decide whether there
        // is a `semantics` member at all; the values vector being absent is
        // `"values": null` -- a member with a null value, not a missing
        // member and not an empty array (deserializer.rs:700).
        if (g->semantics() == nullptr) {
            sem["values"] = nullptr;
        } else {
            sem["values"] = decode_semantics_values(kind_of(g), as_uint_view(g->solids()),
                                                    as_uint_view(g->shells()),
                                                    as_uint_view(g->semantics()));
        }
        out["semantics"] = std::move(sem);
    }

    // Appearance: per-geometry mappings only. The header's `appearance`
    // object (the materials/textures/vertices-texture arrays these index
    // into) is deliberately not emitted -- the Rust reader does not emit it
    // either, and CityJSONSeq consumers read it from the source file.
    //
    // An EMPTY mapping vector omits the key entirely: the reference returns
    // None for an empty slice (geom_decoder.rs:343, :472) and serde drops the
    // field. Every mapping in a NON-empty vector now yields a theme -- an
    // absent `vertices` is a null or absent `values`, not a theme to skip.
    if (g->material() != nullptr && g->material()->size() > 0) {
        out["material"] = materials_to_json(g->material(), kind_of(g));
    }
    if (g->texture() != nullptr && g->texture()->size() > 0) {
        out["texture"] = textures_to_json(g->texture(), kind_of(g));
    }

    return out;
}

/// The `address` sub-object of `pointOfContact`, or null.
///
/// Mirrors `to_cj_address` (deserializer.rs:172-182): emitted only when
/// ALL FIVE fields are present -- including a thoroughfare number that
/// parses as a whole integer -- because the Rust side chains `?`/`and_then`
/// across every field and yields None the moment any one is missing or
/// unparseable. A reader must not abort on a malformed number here, so an
/// unparseable one is treated the same as absent: the address is omitted,
/// not the whole metadata line.
nlohmann::json point_of_contact_address_to_json(const FileInfo& info) {
    if (info.poc_address_thoroughfare_name.empty() || info.poc_address_locality.empty() ||
        info.poc_address_postcode.empty() || info.poc_address_country.empty()) {
        return nullptr;
    }
    const auto& s = info.poc_address_thoroughfare_number;
    long long n = 0;
    const auto res = std::from_chars(s.data(), s.data() + s.size(), n);
    if (res.ec != std::errc() || res.ptr != s.data() + s.size()) return nullptr;

    nlohmann::json addr = nlohmann::json::object();
    addr["thoroughfareNumber"] = n;
    addr["thoroughfareName"] = info.poc_address_thoroughfare_name;
    addr["locality"] = info.poc_address_locality;
    addr["postalCode"] = info.poc_address_postcode;
    addr["country"] = info.poc_address_country;
    return addr;
}

/// `pointOfContact`, or null when absent.
///
/// Mirrors `to_cj_point_of_contact` (deserializer.rs:77-78, :166-182):
/// presence hinges on `poc_contact_name` alone, matching the Rust reader's
/// `match header.poc_contact_name() { Some(_) => ..., None => None }`.
///
/// `emailAddress` is NOT like the other optional fields below: cjseq2's
/// `PointOfContact::email_address` is a required `String` (no
/// `skip_serializing_if`), and `to_cj_point_of_contact` (deserializer.rs:
/// 175-177) does `.ok_or(Error::MissingRequiredField("email_address"))?`,
/// which propagates out of the whole `to_cj_metadata` call. So a header with
/// `poc_contact_name` set but `poc_email` absent is not "pointOfContact minus
/// emailAddress" on the Rust side -- it is a hard failure for the entire
/// metadata line. C++ must match that rather than silently emitting an
/// incomplete object.
///
/// The gate below is on `info.has_poc_email`, NOT `info.poc_email.empty()`.
/// Rust's check is `header.poc_email().ok_or(...)`  a present-but-empty
/// flatbuffer string is `Some("")`, which satisfies `ok_or` and yields
/// `email_address: ""`; only a genuinely absent field is `None` and throws.
/// `poc_email.empty()` cannot make that distinction on its own, since both
/// "absent" and "present but empty" flatten to the same empty
/// `std::string`, so a separate presence flag is required here.
nlohmann::json point_of_contact_to_json(const FileInfo& info) {
    if (info.poc_contact_name.empty()) return nullptr;
    if (!info.has_poc_email) {
        throw Error(ErrorCode::MissingRequiredField, "email_address");
    }

    nlohmann::json poc = nlohmann::json::object();
    poc["contactName"] = info.poc_contact_name;
    if (!info.poc_contact_type.empty()) poc["contactType"] = info.poc_contact_type;
    if (!info.poc_role.empty()) poc["role"] = info.poc_role;
    if (!info.poc_phone.empty()) poc["phone"] = info.poc_phone;
    poc["emailAddress"] = info.poc_email;
    if (!info.poc_website.empty()) poc["website"] = info.poc_website;
    if (auto addr = point_of_contact_address_to_json(info); !addr.is_null()) {
        poc["address"] = std::move(addr);
    }
    return poc;
}

/// The header line's top-level `extensions`: name -> {url, version}.
///
/// Mirrors `to_cj_metadata`'s extensions block (deserializer.rs:33-49): an
/// entry without a name is skipped (a HashMap key cannot be absent), later
/// entries win on a duplicate name (HashMap::insert semantics), and the key
/// itself is omitted when the header carries no extensions vector or every
/// entry lacked a name -- not merely when the vector is empty, matching
/// `if !extensions_map.is_empty()`.
nlohmann::json extensions_to_json(const ::Header* hdr) {
    if (hdr == nullptr || hdr->extensions() == nullptr) return nullptr;

    nlohmann::json out = nlohmann::json::object();
    for (const auto* e : *hdr->extensions()) {
        if (e == nullptr || e->name() == nullptr) continue;
        nlohmann::json entry = nlohmann::json::object();
        entry["url"] = (e->url() != nullptr) ? e->url()->str() : "";
        entry["version"] = (e->version() != nullptr) ? e->version()->str() : "";
        out[e->name()->str()] = std::move(entry);
    }
    return out.empty() ? nlohmann::json(nullptr) : out;
}

}  // namespace

std::string city_object_type_name(std::uint8_t type) {
    constexpr std::size_t kCount =
        sizeof(kCityObjectTypeNames) / sizeof(kCityObjectTypeNames[0]);
    // UNKNOWN-TAG POLICY. ExtensionObject (the enumerator just past the table)
    // and any tag a newer encoder may add come back as "+UnknownCityObject".
    // This used to throw, which meant a file whose ExtensionObject had lost
    // its `extension_type` string was unreadable here while the Rust reader
    // read it happily. It is only ever reached when `extension_type` is
    // absent -- when it is present, its verbatim string wins.
    if (type >= kCount) return kUnknownCityObjectName;
    return kCityObjectTypeNames[type];
}

std::string semantic_surface_type_name(std::uint8_t type) {
    constexpr std::size_t kCount =
        sizeof(kSemanticSurfaceTypeNames) / sizeof(kSemanticSurfaceTypeNames[0]);
    // UNKNOWN-TAG POLICY, as for city_object_type_name above.
    // ExtraSemanticSurface, the enumerator just past the table, and anything a
    // newer encoder adds come back as "+GenericSurface" -- the same string the
    // Rust reader emits (geom_decoder.rs::to_cj_surface_type). Only reached
    // when the surface's `extension_type` string is absent.
    if (type >= kCount) return kGenericSurfaceName;
    return kSemanticSurfaceTypeNames[type];
}

nlohmann::json to_cityjson_metadata(const HeaderView& header) {
    const auto& info = header.info();
    nlohmann::json cj = nlohmann::json::object();

    cj["type"] = "CityJSON";
    cj["version"] = info.cityjson_version;

    // `transform` is unconditional on the Rust side: `to_cj_metadata` starts
    // from `CityJSON::new()`, whose `Transform::new()` defaults to
    // scale [1,1,1] / translate [0,0,0] (cjseq2 lib.rs:1057-1064), and only
    // overwrites it when `header.transform()` is `Some` (deserializer.rs:
    // 24-31). Rust never omits the key, so this must not gate on
    // `has_transform` either -- same "unconditional, not conditional" class
    // as `metadata`/`geographicalExtent`/`extensions` above.
    cj["transform"] = {
        {"scale", info.has_transform ? info.scale : std::array<double, 3>{1.0, 1.0, 1.0}},
        {"translate", info.has_transform ? info.translate : std::array<double, 3>{0.0, 0.0, 0.0}}};

    // `metadata` and `metadata.geographicalExtent` are UNCONDITIONAL on the
    // Rust side (deserializer.rs:81-90): `cj.metadata` is always
    // `Some(CjMetadata { geographical_extent: Some(...), ... })`, defaulting
    // the extent to six zeros via `.unwrap_or_default()` when the header
    // carries none, rather than omitting the field. Confirmed empirically
    // on noise_extension.fcb, whose header has no GeographicalExtent at all
    // yet whose Rust-reader output still carries
    // `"metadata":{"geographicalExtent":[0,0,0,0,0,0]}` -- a gap the
    // previously-narrowed metadata comparison in test_conformance.cpp
    // masked. Every other field stays conditional, matching the `Option`s
    // in CjMetadata.
    nlohmann::json meta = nlohmann::json::object();
    meta["geographicalExtent"] =
        info.has_extent ? info.geographical_extent : std::array<double, 6>{};
    if (!info.crs.empty()) {
        meta["referenceSystem"] =
            "https://www.opengis.net/def/crs/" +
            info.crs.substr(0, info.crs.find(':')) + "/0/" +
            info.crs.substr(info.crs.find(':') + 1);
    }
    if (!info.identifier.empty()) meta["identifier"] = info.identifier;
    if (auto poc = point_of_contact_to_json(info); !poc.is_null()) {
        meta["pointOfContact"] = std::move(poc);
    }
    if (!info.reference_date.empty()) meta["referenceDate"] = info.reference_date;
    if (!info.title.empty()) meta["title"] = info.title;
    cj["metadata"] = std::move(meta);

    // A CityJSONSeq header line carries no features of its own.
    cj["CityObjects"] = nlohmann::json::object();
    cj["vertices"] = nlohmann::json::array();

    // Geometry templates: shapes shared by every GeometryInstance in the
    // file, with their own vertex list. Emitted only when BOTH arrays are
    // present -- a template without vertices indexes nothing
    // (deserializer.rs:92).
    const ::Header* hdr = detail::HeaderAccess::get(header);
    if (auto ext = extensions_to_json(hdr); !ext.is_null()) {
        cj["extensions"] = std::move(ext);
    }

    if (hdr != nullptr && hdr->templates() != nullptr &&
        hdr->templates_vertices() != nullptr) {
        auto templates = nlohmann::json::array();
        for (const auto* t : *hdr->templates()) {
            if (t != nullptr) templates.push_back(geometry_to_json(t, info.semantic_columns));
        }

        // Template vertices are absolute doubles, NOT quantised: the header
        // transform does not apply to them. Read via memcpy for the same
        // reason as Transform -- the struct can sit at a misaligned offset.
        auto verts = nlohmann::json::array();
        for (const auto* v : *hdr->templates_vertices()) {
            std::array<double, 3> xyz{};
            for (std::size_t i = 0; i < 3; ++i) {
                std::memcpy(&xyz[i], reinterpret_cast<const std::uint8_t*>(v) + i * sizeof(double),
                            sizeof(double));
            }
            verts.push_back(nlohmann::json::array({xyz[0], xyz[1], xyz[2]}));
        }

        cj["geometry-templates"] = {{"templates", std::move(templates)},
                                    {"vertices-templates", std::move(verts)}};
    }

    return cj;
}

nlohmann::json to_cityjson_feature(const Feature& feature, const HeaderView& header) {
    const ::CityFeature* cf = detail::FeatureAccess::get(feature);
    if (cf == nullptr) {
        throw Error(ErrorCode::MissingRequiredField, "empty feature");
    }

    nlohmann::json out = nlohmann::json::object();
    out["type"] = "CityJSONFeature";
    out["id"] = feature.id();

    nlohmann::json objects = nlohmann::json::object();
    const std::size_t n = feature.city_object_count();
    for (std::size_t i = 0; i < n; ++i) {
        const auto* obj = cf->objects()->Get(static_cast<flatbuffers::uoffset_t>(i));
        if (obj == nullptr) continue;

        nlohmann::json co = nlohmann::json::object();
        co["type"] = (obj->extension_type() != nullptr)
                         ? obj->extension_type()->str()
                         : city_object_type_name(static_cast<std::uint8_t>(obj->type()));

        // Per-object schema when declared, header schema otherwise.
        // Emitted iff the object DECLARES an attributes vector -- a
        // present-but-empty one becomes `{}`, an absent one is omitted
        // entirely. The reference distinguishes these and consumers compare
        // against it.
        if (feature.object_has_attributes(i)) {
            auto own = feature.object_columns(i);
            auto blob = feature.object_attributes(i);
            const auto& schema =
                feature.object_has_columns(i) ? own : header.info().columns;
            co["attributes"] = blob.empty() ? nlohmann::json::object()
                                            : attributes_to_json(blob, schema);
        }

        std::array<double, 6> extent{};
        if (feature.object_extent(i, extent)) {
            co["geographicalExtent"] = extent;
        }

        auto geoms = nlohmann::json::array();
        if (obj->geometry() != nullptr) {
            for (const auto* g : *obj->geometry()) {
                if (g != nullptr) {
                    geoms.push_back(geometry_to_json(g, header.info().semantic_columns));
                }
            }
        }
        // Geometry templates: the shape lives once in the header and each
        // instance references it by index plus a 4x4 placement matrix and a
        // single reference-point vertex.
        if (obj->geometry_instances() != nullptr) {
            for (const auto* gi : *obj->geometry_instances()) {
                if (gi != nullptr) geoms.push_back(geometry_instance_to_json(gi));
            }
        }
        if (!geoms.empty()) co["geometry"] = std::move(geoms);

        if (obj->children() != nullptr && obj->children()->size() > 0) {
            auto kids = nlohmann::json::array();
            for (const auto* c : *obj->children()) {
                if (c != nullptr) kids.push_back(c->str());
            }
            co["children"] = std::move(kids);
        }
        if (obj->parents() != nullptr && obj->parents()->size() > 0) {
            auto ps = nlohmann::json::array();
            for (const auto* p : *obj->parents()) {
                if (p != nullptr) ps.push_back(p->str());
            }
            co["parents"] = std::move(ps);
        }

        objects[feature.object_id(i)] = std::move(co);
    }
    out["CityObjects"] = std::move(objects);

    // Vertices are quantised integers; the header transform maps them back
    // to world coordinates, so they stay integral here.
    auto verts = nlohmann::json::array();
    if (cf->vertices() != nullptr) {
        for (const auto* v : *cf->vertices()) {
            verts.push_back(nlohmann::json::array({v->x(), v->y(), v->z()}));
        }
    }
    out["vertices"] = std::move(verts);

    // The materials, textures and UV vertices this feature's geometry
    // mappings index into. Without it the mappings reference nothing a
    // consumer can resolve (deserializer.rs:503).
    if (cf->appearance() != nullptr) {
        out["appearance"] = appearance_to_json(cf->appearance());
    }

    return out;
}

}  // namespace fcb

#endif  // FCB_WITH_JSON
