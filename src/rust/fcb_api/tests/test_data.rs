// Expected responses from the actual 3DBAG API
// These are used to ensure our API matches the original structure

pub const EXPECTED_LANDING_PAGE: &str = r#"{
  "description": "3DBAG is an extended version of the 3DBAG data set. It contains additional information that is either derived from the 3DBAG, or integrated from other data sources.",
  "links": [
    {
      "href": "https://api.3dbag.nl/",
      "rel": "self",
      "title": "this document",
      "type": "application/json"
    },
    {
      "href": "https://api.3dbag.nl/api",
      "rel": "service-desc",
      "title": "the API definition",
      "type": "application/vnd.oai.openapi+json;version=3.0"
    },
    {
      "href": "https://api.3dbag.nl/api.html",
      "rel": "service-doc",
      "title": "the API documentation",
      "type": "text/html"
    },
    {
      "href": "https://api.3dbag.nl/conformance",
      "rel": "conformance",
      "title": "Conformance classes implemented by this server",
      "type": "application/json"
    },
    {
      "href": "https://api.3dbag.nl/collections",
      "rel": "data",
      "title": "Information about the feature collections",
      "type": "application/json"
    }
  ],
  "title": "3DBAG API"
}"#;

pub const EXPECTED_CONFORMANCE: &str = r#"{
  "conformsTo": [
    "https://cityjson.org/specs/1.1.1/"
  ]
}"#;

pub const EXPECTED_COLLECTIONS: &str = r#"{
  "collections": [
    {
      "crs": [
        "http://www.opengis.net/def/crs/EPSG/0/7415"
      ],
      "description": "3D building models based on the 'pand' layer of the BAG dataset.",
      "extent": {
        "spatial": {
          "bbox": [
            [10000, 306250, 287760, 623690]
          ],
          "crs": "http://www.opengis.net/def/crs/EPSG/0/7415"
        }
      },
      "id": "pand",
      "itemType": "feature",
      "links": [
        {
          "href": "https://api.3dbag.nl/collections/pand",
          "rel": "self",
          "title": "this document",
          "type": "application/json"
        },
        {
          "href": "https://api.3dbag.nl/collections/pand/items",
          "rel": "items",
          "title": "Pand items",
          "type": "application/geo+json"
        },
        {
          "href": "https://creativecommons.org/licenses/by/4.0/",
          "rel": "license",
          "title": "CC BY 4.0",
          "type": "text/html"
        },
        {
          "href": "https://creativecommons.org/licenses/by/4.0/rdf",
          "rel": "license",
          "title": "CC BY 4.0",
          "type": "application/rdf+xml"
        }
      ],
      "storageCrs": "http://www.opengis.net/def/crs/EPSG/0/7415",
      "title": "Pand",
      "version": {
        "api": "0.1",
        "collection": "v2023.10.08"
      }
    }
  ],
  "crs": [
    "http://www.opengis.net/def/crs/EPSG/0/7415"
  ],
  "links": [
    {
      "href": "https://api.3dbag.nl/collections",
      "rel": "self",
      "title": "this document",
      "type": "application/json"
    }
  ]
}"#;

pub const EXPECTED_COLLECTION_PAND: &str = r#"{
  "crs": [
    "http://www.opengis.net/def/crs/EPSG/0/7415"
  ],
  "description": "3D building models based on the 'pand' layer of the BAG dataset.",
  "extent": {
    "spatial": {
      "bbox": [
        [10000.0, 306250.0, 287760.0, 623690.0]
      ],
      "crs": "http://www.opengis.net/def/crs/EPSG/0/7415"
    }
  },
  "id": "pand",
  "itemType": "feature",
  "links": [
    {
      "href": "https://api.3dbag.nl/collections/pand",
      "rel": "self",
      "title": "this document",
      "type": "application/json"
    },
    {
      "href": "https://api.3dbag.nl/collections/pand/items",
      "rel": "items",
      "title": "Pand items",
      "type": "application/geo+json"
    },
    {
      "href": "https://creativecommons.org/licenses/by/4.0/",
      "rel": "license",
      "title": "CC BY 4.0",
      "type": "text/html"
    },
    {
      "href": "https://creativecommons.org/licenses/by/4.0/rdf",
      "rel": "license",
      "title": "CC BY 4.0",
      "type": "application/rdf+xml"
    }
  ],
  "storageCrs": "http://www.opengis.net/def/crs/EPSG/0/7415",
  "title": "Pand",
  "version": {
    "api": "0.1",
    "collection": "v2023.10.08"
  },
  "cityjson": {
    "version": "2.0",
    "transform": {
      "scale": [0.001, 0.001, 0.001],
      "translate": [171800.0, 472700.0, 0.0]
    },
    "extensions": []
  }
}"#;

pub const EXPECTED_ITEM_BY_ID: &str = r#"{
  "feature": {
    "CityObjects": {
      "NL.IMBAG.Pand.0851100000000564": {
        "attributes": {
          "b3_bag_bag_overlap": null,
          "b3_bouwlagen": null,
          "b3_dak_type": "slanted",
          "b3_extrusie": 0,
          "b3_h_dak_50p": 3.70600008964539,
          "b3_h_dak_70p": 4.0770001411438,
          "b3_h_dak_max": 4.68300008773804,
          "b3_h_dak_min": 2.53999996185303,
          "b3_h_maaiveld": 1.71300005912781,
          "b3_is_glas_dak": false,
          "b3_kas_warenhuis": false,
          "b3_mutatie_AHN3_AHN4": false,
          "b3_mutatie_AHN4_AHN5": false,
          "b3_n_vlakken": 2,
          "b3_nodata_fractie_AHN3": 0.028985507786274,
          "b3_nodata_fractie_AHN4": 0.202898547053337,
          "b3_nodata_fractie_AHN5": 0.177536234259605,
          "b3_nodata_radius_AHN3": 0.522119045257568,
          "b3_nodata_radius_AHN4": 1.52207124233246,
          "b3_nodata_radius_AHN5": 0.854065239429474,
          "b3_opp_buitenmuur": 44.56,
          "b3_opp_dak_plat": 0,
          "b3_opp_dak_schuin": 77.2,
          "b3_opp_grond": 68.87,
          "b3_opp_scheidingsmuur": 0,
          "b3_puntdichtheid_AHN3": 8.04477596282959,
          "b3_puntdichtheid_AHN4": 15.6000003814697,
          "b3_puntdichtheid_AHN5": 14.8722467422485,
          "b3_pw_bron": "AHN3",
          "b3_pw_datum": 2017,
          "b3_pw_onvoldoende": false,
          "b3_pw_selectie_reden": "_HIGHEST_YET_INSUFFICIENT_COVERAGE",
          "b3_rmse_lod12": 0.666661679744721,
          "b3_rmse_lod13": 0.666661679744721,
          "b3_rmse_lod22": 0.232827037572861,
          "b3_succes": true,
          "b3_t_run": 35,
          "b3_val3dity_lod12": "[]",
          "b3_val3dity_lod13": "[]",
          "b3_val3dity_lod22": "[]",
          "b3_volume_lod12": 156.445449829102,
          "b3_volume_lod13": 156.445449829102,
          "b3_volume_lod22": 128.232803344727,
          "begingeldigheid": "2011-06-06",
          "documentdatum": "2011-06-06",
          "documentnummer": "BM1100602",
          "eindgeldigheid": null,
          "eindregistratie": null,
          "fid": 16296286,
          "geconstateerd": false,
          "identificatie": "NL.IMBAG.Pand.0851100000000564",
          "oorspronkelijkbouwjaar": 1920,
          "rf_force_lod11": false,
          "status": "Pand in gebruik",
          "tijdstipeindregistratielv": null,
          "tijdstipinactief": null,
          "tijdstipinactieflv": null,
          "tijdstipnietbaglv": null,
          "tijdstipregistratie": "2011-06-07T09:31:44Z",
          "tijdstipregistratielv": "2011-06-07T10:01:30Z",
          "voorkomenidentificatie": 2
        },
        "children": [
          "NL.IMBAG.Pand.0851100000000564-0"
        ],
        "geometry": [
          {
            "boundaries": [
              [
                [0, 1, 2, 3]
              ]
            ],
            "lod": "0",
            "type": "MultiSurface"
          }
        ],
        "type": "Building"
      },
      "NL.IMBAG.Pand.0851100000000564-0": {
        "geometry": [
          {
            "boundaries": [
              [
                [
                  [4, 5, 6, 7]
                ],
                [
                  [8, 6, 5, 9]
                ],
                [
                  [10, 7, 6, 8]
                ],
                [
                  [11, 4, 7, 10]
                ],
                [
                  [9, 5, 4, 11]
                ],
                [
                  [9, 11, 10, 8]
                ]
              ]
            ],
            "lod": "1.2",
            "semantics": {
              "surfaces": [
                {
                  "type": "GroundSurface"
                },
                {
                  "on_footprint_edge": true,
                  "type": "WallSurface"
                },
                {
                  "on_footprint_edge": false,
                  "type": "WallSurface"
                },
                {
                  "b3_h_dak_50p": 3.54450392723084,
                  "b3_h_dak_70p": 3.95685529708862,
                  "b3_h_dak_max": 4.79144620895386,
                  "b3_h_dak_min": 2.29176235198975,
                  "type": "RoofSurface"
                }
              ],
              "values": [
                [0, 1, 1, 1, 1, 3]
              ]
            },
            "type": "Solid"
          },
          {
            "boundaries": [
              [
                [
                  [4, 5, 6, 7]
                ],
                [
                  [8, 6, 5, 9]
                ],
                [
                  [10, 7, 6, 8]
                ],
                [
                  [11, 4, 7, 10]
                ],
                [
                  [9, 5, 4, 11]
                ],
                [
                  [9, 11, 10, 8]
                ]
              ]
            ],
            "lod": "1.3",
            "semantics": {
              "surfaces": [
                {
                  "type": "GroundSurface"
                },
                {
                  "on_footprint_edge": true,
                  "type": "WallSurface"
                },
                {
                  "on_footprint_edge": false,
                  "type": "WallSurface"
                },
                {
                  "b3_h_dak_50p": 3.54450392723084,
                  "b3_h_dak_70p": 3.95685529708862,
                  "b3_h_dak_max": 4.79144620895386,
                  "b3_h_dak_min": 2.29176235198975,
                  "type": "RoofSurface"
                }
              ],
              "values": [
                [0, 1, 1, 1, 1, 3]
              ]
            },
            "type": "Solid"
          },
          {
            "boundaries": [
              [
                [
                  [7, 12, 4, 5, 13, 6]
                ],
                [
                  [14, 13, 5, 15]
                ],
                [
                  [16, 6, 13, 14]
                ],
                [
                  [17, 7, 6, 16]
                ],
                [
                  [15, 5, 4, 18]
                ],
                [
                  [18, 4, 12, 19]
                ],
                [
                  [19, 12, 7, 17]
                ],
                [
                  [19, 14, 15, 18]
                ],
                [
                  [14, 19, 17, 16]
                ]
              ]
            ],
            "lod": "2.2",
            "semantics": {
              "surfaces": [
                {
                  "type": "GroundSurface"
                },
                {
                  "on_footprint_edge": true,
                  "type": "WallSurface"
                },
                {
                  "on_footprint_edge": false,
                  "type": "WallSurface"
                },
                {
                  "b3_azimut": 48.6910705566406,
                  "b3_h_dak_50p": 3.60037326812744,
                  "b3_h_dak_70p": 4.00212097167969,
                  "b3_h_dak_max": 4.79144620895386,
                  "b3_h_dak_min": 2.62147736549377,
                  "b3_hellingshoek": 26.4350967407227,
                  "type": "RoofSurface"
                },
                {
                  "b3_azimut": 230.800704956055,
                  "b3_h_dak_50p": 3.57543611526489,
                  "b3_h_dak_70p": 4.04714727401733,
                  "b3_h_dak_max": 4.79144620895386,
                  "b3_h_dak_min": 2.29176235198975,
                  "b3_hellingshoek": 27.198600769043,
                  "type": "RoofSurface"
                }
              ],
              "values": [
                [0, 1, 1, 1, 1, 1, 1, 3, 4]
              ]
            },
            "type": "Solid"
          }
        ],
        "parents": [
          "NL.IMBAG.Pand.0851100000000564"
        ],
        "type": "BuildingPart"
      }
    },
    "id": "NL.IMBAG.Pand.0851100000000564",
    "type": "CityJSONFeature",
    "vertices": [
      [0, 6422, 0],
      [5819, 0, 0],
      [11659, 5329, 0],
      [6083, 11789, 0],
      [6083, 11789, 1713],
      [11659, 5329, 1713],
      [5819, 0, 1713],
      [0, 6422, 1713],
      [5819, 0, 3984],
      [11659, 5329, 3984],
      [0, 6422, 3984],
      [6083, 11789, 3984],
      [3373, 9399, 1713],
      [8926, 2836, 1713],
      [8926, 2836, 4638],
      [11659, 5329, 2798],
      [5819, 0, 2480],
      [0, 6422, 2247],
      [6083, 11789, 2761],
      [3373, 9399, 4558]
    ]
  },
  "id": "NL.IMBAG.Pand.0851100000000564",
  "links": [
    {
      "href": "https://api.3dbag.nl/collections/pand/items/NL.IMBAG.Pand.0851100000000564",
      "rel": "self",
      "title": "this document",
      "type": "application/json"
    },
    {
      "href": "https://api.3dbag.nl/collections/pand",
      "rel": "collection",
      "type": "application/json"
    },
    {
      "href": "https://api.3dbag.nl/collections/pand/items/NL.IMBAG.Pand.0851100000000564",
      "rel": "parent",
      "type": "application/city+json"
    },
    {
      "href": "https://api.3dbag.nl/collections/pand/items/NL.IMBAG.Pand.0851100000000564-0",
      "rel": "child",
      "type": "application/city+json"
    }
  ],
  "metadata": {
    "CityObjects": {

    },
    "metadata": {
      "referenceSystem": "https://www.opengis.net/def/crs/EPSG/0/7415"
    },
    "transform": {
      "scale": [0.001, 0.001, 0.001],
      "translate": [89379.4295, 398420.33425, 0.000002227783198804]
    },
    "type": "CityJSON",
    "version": "2.0",
    "vertices": []
  }
}"#;
