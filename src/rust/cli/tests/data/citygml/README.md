# CityGML test data

Copied unmodified from the CityGML crates' own test data in
[cityjson/cityparquet](https://github.com/cityjson/cityparquet) at `fd25995`
(`lib/citygml-rs/crates/citygml{2,3}/tests/data/`, whose READMEs give the full
provenance).

| File | Source |
| --- | --- |
| `53394610_brid_6697_op.gml` | [PLATEAU](https://www.mlit.go.jp/plateau/) Chiyoda-ku 2025, `udx/brid/`, CityGML 2.0. MLIT, under the [PLATEAU site policy](https://www.mlit.go.jp/plateau/site-policy/), compatible with CC BY 4.0. |
| `53394610_brid_6697_op.v3.gml` | The same file after `citygml-tools upgrade -l -s -r` (citygml-tools 2.5.0): CityGML 3.0. |
| `railway_trees_fragment.gml` | `citygml-tools from-cityjson` (2.5.0) on cityjson.org's LoD 3 Railway (after the KIT/IAI Railway reference dataset, free for unrestricted use), cut to the CityObjectGroup, fifteen trees with `ImplicitGeometry`, and the CityModel-level appearance. |
