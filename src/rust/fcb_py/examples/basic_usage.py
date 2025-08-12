#!/usr/bin/env python3
"""
Basic usage example for FlatCityBuf Python bindings.

This example demonstrates the core functionality of the FCB Python bindings,
including reading files, querying features, and accessing CityJSON data.
Updated to reflect the latest CityJSON integration and async functionality.
"""

import asyncio

import flatcitybuf as fcb


def demonstrate_sync_reader():
    """Demonstrate synchronous reader for local files"""
    print("=== Synchronous Reader (Local Files) ===\n")

    # For this example, we'll use a hypothetical file path
    # In practice, replace with path to an actual .fcb file
    fcb_file = "example_data/delft.fcb"

    print(f"Attempting to read: {fcb_file}")

    try:
        # Create reader
        reader = fcb.Reader(fcb_file)
        print("✓ Reader created successfully")

        # Get file information
        info = reader.info()
        print("\nFile Information:")
        print(f"  Features: {info.feature_count}")
        print(f"  CRS: {info.crs or 'Not specified'}")
        if info.bbox:
            print(f"  Bounding box: {info.bbox}")

        # Get CityJSON header with transform and metadata
        cityjson = reader.cityjson_header()
        print("\nCityJSON Header:")
        print(f"  Type: {cityjson.type}")
        print(f"  Version: {cityjson.version}")
        print(f"  Feature Count: {cityjson.feature_count}")
        print(f"  Transform Scale: {cityjson.transform.scale}")
        print(f"  Transform Translate: {cityjson.transform.translate}")
        if cityjson.metadata:
            print(f"  Metadata Title: {cityjson.metadata.title}")

        # Example: Iterate through features (CityJSON format)
        print("\n=== Feature Iteration (CityJSON Format) ===")
        feature_count = 0
        for feature in reader:
            print(f"Feature {feature_count + 1}:")
            print(f"  ID: {feature.id}")
            print(f"  Type: {feature.type}")
            print(f"  Vertices: {len(feature.vertices)} vertices")
            print(f"  City Objects: {len(feature.city_objects)} objects")

            # Show first city object
            if feature.city_objects:
                first_obj_id = next(iter(feature.city_objects.keys()))
                first_obj = feature.city_objects[first_obj_id]
                print(f"    First object ID: {first_obj_id}")
                print(f"    First object type: {first_obj.type}")
                print(f"    Geometries: {len(first_obj.geometry)}")

                # Show geometry with nested boundaries
                if first_obj.geometry:
                    geom = first_obj.geometry[0]
                    print(f"      Geometry type: {geom.type}")
                    print(f"      Boundary structure: {type(geom.boundaries)}")

            feature_count += 1

            # Limit output for demo
            if feature_count >= 2:
                print("  ... (showing first 2 features only)")
                break

        # Example: Spatial queries
        print("\n=== Spatial Query ===")
        # Using Delft coordinates from our test
        minx = 84227.77
        miny = 445377.33
        maxx = 85323.23
        maxy = 446334.69

        spatial_features = list(reader.query_bbox(minx, miny, maxx, maxy))
        print(f"Found {len(spatial_features)} features in bounding box")

        for i, feature in enumerate(spatial_features[:2]):  # Show first 2
            print(f"  Feature {i + 1}: {feature.id}")

        # Example: Attribute queries
        print("\n=== Attribute Query ===")

        try:
            # Query for specific building by ID (from our test data)
            id_filter = fcb.AttrFilter(
                "identificatie",
                fcb.Operator.Eq,
                "NL.IMBAG.Pand.0503100000012869",
            )
            buildings = list(reader.query_attr([id_filter]))
            print(f"Found {len(buildings)} features with specific ID")
            if buildings:
                print(f"  Found feature: {buildings[0].id}")
        except fcb.FcbError as e:
            print(f"Attribute query not available: {e}")

        print("\n✓ Synchronous operations completed successfully")

    except fcb.FcbError as e:
        print(f"FCB Error: {e}")
        print("\nThis is expected if no test .fcb file is available.")
        print("To test with real data:")
        print(
            "1. Create a .fcb file using the CLI: cargo run -p fcb_cli ser -i data.city.jsonl -o data.fcb"
        )
        print("2. Update the fcb_file path in this script")

    except FileNotFoundError:
        print(f"File not found: {fcb_file}")
        print("\nTo run this example with real data:")
        print("1. Create test data using the FlatCityBuf CLI")
        print("2. Update the file path in this script")

    except Exception as e:
        print(f"Unexpected error: {e}")


async def demonstrate_async_reader():
    """Demonstrate asynchronous reader for HTTP access"""
    print("\n=== Asynchronous Reader (HTTP Access) ===\n")

    # Example HTTP URL (replace with actual FCB file URL)
    http_url = "https://storage.googleapis.com/flatcitybuf/delft.city.fcb"

    print(f"Attempting to read from HTTP: {http_url}")

    try:
        # Create async reader
        async_reader = fcb.AsyncReader(http_url)
        print("✓ AsyncReader created successfully")

        # Open the reader (establishes HTTP connection)
        opened_reader = await async_reader.open()
        print("✓ HTTP connection established")

        # Get file information
        info = opened_reader.info()
        print("\nFile Information:")
        print(f"  Features: {info.feature_count}")
        print(f"  CRS: {info.crs or 'Not specified'}")
        if info.bbox:
            print(f"  Bounding box: {info.bbox}")

        # Get CityJSON header
        cityjson = opened_reader.cityjson_header()
        print("\nCityJSON Header:")
        print(f"  Type: {cityjson.type}")
        print(f"  Version: {cityjson.version}")
        print(f"  Feature Count: {cityjson.feature_count}")

        # Example: Async iterator for all features
        print("\n=== Async Feature Iteration ===")
        async_iter = opened_reader.select_all()

        count = 0
        for _ in range(3):  # Get first 3 features
            feature = await async_iter.next()
            if feature is None:
                break
            print(f"Feature {count + 1}:")
            print(f"  ID: {feature.id}")
            print(f"  Type: {feature.type}")
            print(f"  City Objects: {len(feature.city_objects)} objects")
            count += 1

        # Example: Collect all features at once
        print("\n=== Async Collect All Features ===")
        all_iter = opened_reader.select_all()
        all_features = await all_iter.collect()
        print(f"Collected {len(all_features)} features total")

        # Example: Async spatial query
        print("\n=== Async Spatial Query ===")
        minx = 84227.77
        miny = 445377.33
        maxx = 85323.23
        maxy = 446334.69

        bbox_iter = opened_reader.query_bbox(minx, miny, maxx, maxy)
        bbox_features = await bbox_iter.collect()
        print(f"Found {len(bbox_features)} features in bounding box")

        # Example: Async iterator for spatial query
        print("\n=== Async Spatial Iterator ===")
        spatial_iter = opened_reader.query_bbox(minx, miny, maxx, maxy)

        count = 0
        while count < 2:  # Get first 2 features
            feature = await spatial_iter.next()
            if feature is None:
                break
            print(f"  Spatial feature {count + 1}: {feature.id}")
            count += 1

        # Example: Async attribute query
        print("\n=== Async Attribute Query ===")
        try:
            id_filter = fcb.AttrFilter(
                "identificatie",
                fcb.Operator.Eq,
                "NL.IMBAG.Pand.0503100000012869",
            )
            attr_iter = opened_reader.query_attr([id_filter])
            attr_features = await attr_iter.collect()
            print(f"Found {len(attr_features)} features with specific ID")
            if attr_features:
                print(f"  Found feature: {attr_features[0].id}")
        except fcb.FcbError as e:
            print(f"Attribute query failed: {e}")

        print("\n✓ Asynchronous operations completed successfully")

    except fcb.FcbError as e:
        print(f"FCB Error: {e}")
        print("This is expected if the HTTP URL is not accessible.")

    except Exception as e:
        print(f"Unexpected HTTP error: {e}")
        print("This is expected if no internet connection is available.")


def demonstrate_api_features():
    """Demonstrate API features without requiring actual files"""

    print("\n=== API Feature Demonstration ===")

    # Demonstrate type creation
    print("Creating data types...")

    # BBox
    bbox = fcb.BBox(0, 0, 1000, 1000)
    print(
        f"BBox: min_x={bbox.min_x}, min_y={bbox.min_y}, max_x={bbox.max_x}, max_y={bbox.max_y}"
    )

    # AttrFilter examples with updated API
    filters = [
        fcb.AttrFilter("type", fcb.Operator.Eq, "building"),
        fcb.AttrFilter("height", fcb.Operator.Gt, 50.0),
        fcb.AttrFilter("floor_count", fcb.Operator.Le, 10),
        fcb.AttrFilter("status", fcb.Operator.Ne, "demolished"),
    ]

    print("\nAttribute filters:")
    for filt in filters:
        print(f"  {filt.field} {filt.operator} {filt.value}")

    # Convenience functions
    print("\nConvenience functions available:")
    print("  fcb.open_file() - Open file reader and get all features")
    print("  fcb.query_bbox() - Quick spatial query on file")

    # Module info
    print("\nModule classes:")
    print("  Reader - Synchronous reader for local files")
    print("  AsyncReader - Asynchronous reader for HTTP files")
    print("  AttrFilter - Attribute filter for queries")
    print("  BBox - Bounding box for spatial queries")
    print("  Operator - Comparison operators (Eq, Ne, Gt, Ge, Lt, Le)")


async def main():
    """Main example function"""
    print("=== FlatCityBuf Python Bindings Example ===\n")
    print(
        "Demonstrates both local file access and HTTP access with CityJSON support\n"
    )

    # Demonstrate synchronous reader
    demonstrate_sync_reader()

    # Demonstrate asynchronous reader
    await demonstrate_async_reader()

    # Demonstrate API features
    demonstrate_api_features()

    print("\n=== Example Complete ===")
    print("FlatCityBuf Python bindings support both local and HTTP access")
    print("with full CityJSON integration and efficient async iteration.")


if __name__ == "__main__":
    asyncio.run(main())
