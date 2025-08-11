#!/usr/bin/env python3
"""
Basic usage example for FlatCityBuf Python bindings.

This example demonstrates the core functionality of the FCB Python bindings,
including reading files, querying features, and accessing geometry data.
"""

import sys
from pathlib import Path
import flatcitybuf as fcb


def main():
    """Main example function"""

    # Example 1: Basic file reading
    print("=== FlatCityBuf Python Bindings Example ===\n")

    # For this example, we'll use a hypothetical file path
    # In practice, replace with path to an actual .fcb file
    fcb_file = "example_data/city_model.fcb"

    print(f"Attempting to read: {fcb_file}")

    try:
        # Create reader
        reader = fcb.Reader(fcb_file)
        print("✓ Reader created successfully")

        # Get file information
        info = reader.info()
        print(f"\nFile Information:")
        print(f"  Features: {info.feature_count}")
        print(f"  CRS: {info.crs or 'Not specified'}")
        if info.bbox:
            print(f"  Bounding box: {info.bbox}")

        # Example 2: Iterate through features
        print(f"\n=== Feature Iteration ===")
        feature_count = 0
        for feature in reader:
            print(f"Feature {feature_count + 1}:")
            print(f"  ID: {feature.id}")
            print(f"  Type: {feature.feature_type}")
            print(f"  Geometries: {len(feature.geometry)}")

            # Show first geometry if available
            if feature.geometry:
                geom = feature.geometry[0]
                print(f"    First geometry: {geom.geometry_type}")
                print(f"    Vertices: {len(geom.vertices)}")
                if geom.vertices:
                    v = geom.vertices[0]
                    print(f"    First vertex: ({v.x}, {v.y}, {v.z})")

            feature_count += 1

            # Limit output for demo
            if feature_count >= 3:
                print("  ... (showing first 3 features only)")
                break

        # Example 3: Spatial queries
        print(f"\n=== Spatial Query ===")
        bbox = fcb.BBox(min_x=0, min_y=0, max_x=1000, max_y=1000)
        print(f"Querying bounding box: {bbox}")

        spatial_features = reader.query_bbox(0, 0, 1000, 1000)
        print(f"Found {len(spatial_features)} features in bounding box")

        for i, feature in enumerate(spatial_features[:2]):  # Show first 2
            print(f"  Feature {i+1}: {feature.id} ({feature.feature_type})")

        # Example 4: Attribute queries
        print(f"\n=== Attribute Query ===")

        # Query for buildings with height > 50
        try:
            tall_features = reader.query_attr("building_height", ">", 50.0)
            print(f"Found {len(tall_features)} features with height > 50")
        except fcb.FcbError as e:
            print(f"Attribute query not available: {e}")

        # Using AttrFilter class
        filter_eq = fcb.AttrFilter.eq("building_type", "residential")
        print(f"Created equality filter: {filter_eq}")

        filter_gt = fcb.AttrFilter.gt("floor_count", 5)
        print(f"Created greater-than filter: {filter_gt}")

        # Example 5: Working with geometry
        print(f"\n=== Geometry Details ===")

        # Recreate reader for fresh iteration
        reader2 = fcb.Reader(fcb_file)
        for feature in reader2:
            if feature.geometry:
                geom = feature.geometry[0]
                print(f"Geometry type: {geom.geometry_type}")

                # Vertex analysis
                if geom.vertices:
                    print(f"Vertices ({len(geom.vertices)} total):")
                    for i, vertex in enumerate(geom.vertices[:3]):  # First 3
                        print(f"  {i+1}: {vertex}")

                    # Calculate bounding box
                    if len(geom.vertices) > 0:
                        xs = [v.x for v in geom.vertices]
                        ys = [v.y for v in geom.vertices]
                        zs = [v.z for v in geom.vertices]

                        print(f"Geometry bounds:")
                        print(f"  X: {min(xs):.2f} to {max(xs):.2f}")
                        print(f"  Y: {min(ys):.2f} to {max(ys):.2f}")
                        print(f"  Z: {min(zs):.2f} to {max(zs):.2f}")

                # Boundary information
                if geom.boundaries:
                    print(f"Boundaries: {len(geom.boundaries)} surfaces")
                    for i, boundary in enumerate(geom.boundaries[:2]):
                        print(
                            f"  Surface {i+1}: {len(boundary)} vertex indices"
                        )

                break  # Just analyze first geometry

        print(f"\n=== Success! ===")
        print("FlatCityBuf Python bindings are working correctly.")

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
        sys.exit(1)


def demonstrate_api_features():
    """Demonstrate API features without requiring actual files"""

    print(f"\n=== API Feature Demonstration ===")

    # Demonstrate type creation
    print("Creating data types...")

    # Vertex
    vertex = fcb.Vertex(100.5, 200.3, 15.7)
    print(f"Vertex: {vertex}")
    print(f"Coordinates: {vertex.to_tuple()}")

    # BBox
    bbox = fcb.BBox(0, 0, 1000, 1000)
    print(f"BBox: {bbox}")
    print(f"Area: {bbox.area()}")
    print(f"Contains (500, 500): {bbox.contains(500, 500)}")
    print(f"Contains (1500, 500): {bbox.contains(1500, 500)}")

    # Test intersection
    other_bbox = fcb.BBox(500, 500, 1500, 1500)
    print(f"Intersects with {other_bbox}: {bbox.intersects(other_bbox)}")

    # AttrFilter examples
    filters = [
        fcb.AttrFilter.eq("type", "building"),
        fcb.AttrFilter.gt("height", 50.0),
        fcb.AttrFilter.le("floor_count", 10),
        fcb.AttrFilter.ne("status", "demolished"),
    ]

    print("\nAttribute filters:")
    for filter in filters:
        print(f"  {filter}")

    # Convenience functions
    print(f"\nConvenience functions available:")
    print(f"  fcb.open_file() - Open file reader")
    print(f"  fcb.query_bbox() - Quick spatial query")

    # Module info
    print(f"\nModule information:")
    print(f"  Version: {fcb.__version__}")
    print(f"  Available classes: {', '.join(fcb.__all__)}")


if __name__ == "__main__":
    main()
    demonstrate_api_features()
