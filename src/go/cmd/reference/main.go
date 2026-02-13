// Reference implementation demonstrating every public API of the FlatCityBuf Go bindings.
//
// Build the Rust static library first:
//
//	just build-go-lib
//
// Then run:
//
//	cd src/go && go run cmd/reference/main.go ../../examples/data/delft.fcb
package main

import (
	"encoding/json"
	"fmt"
	"log"
	"os"
	"strings"

	"github.com/cityjson/flatcitybuf-go/fcb"
)

func main() {
	if len(os.Args) < 2 {
		fmt.Fprintf(os.Stderr, "Usage: %s <path-to-fcb-file>\n", os.Args[0])
		os.Exit(1)
	}
	path := os.Args[1]

	demoOpenAndInspect(path)
	demoSelectAll(path)
	demoSelectBBox(path)
	demoFeatureJSON(path)
	demoOwnershipModel(path)
	demoErrorHandling()

	fmt.Println("\n=== All demos complete ===")
}

// ─────────────────────────────────────────────────────────────
// 1. Opening and Inspecting FCB Files
// ─────────────────────────────────────────────────────────────
func demoOpenAndInspect(path string) {
	fmt.Println("=== 1. Opening and Inspecting ===")

	reader, err := fcb.Open(path)
	if err != nil {
		log.Fatalf("Failed to open: %v", err)
	}
	defer reader.Close()

	// Basic file properties
	fmt.Printf("Feature count: %d\n", reader.FeaturesCount())
	fmt.Printf("Has spatial index: %v\n", reader.HasSpatialIndex())

	// CityJSON metadata is returned as a parsed JSON map
	meta, err := reader.CityJSONMetadata()
	if err != nil {
		log.Fatalf("Failed to get metadata: %v", err)
	}

	fmt.Printf("CityJSON type: %v\n", meta["type"])
	fmt.Printf("CityJSON version: %v\n", meta["version"])

	if transform, ok := meta["transform"].(map[string]interface{}); ok {
		fmt.Printf("Transform scale: %v\n", transform["scale"])
		fmt.Printf("Transform translate: %v\n", transform["translate"])
	}

	// Access other metadata fields
	for key := range meta {
		if key != "type" && key != "version" && key != "transform" {
			fmt.Printf("Extra metadata key: %s\n", key)
		}
	}
	fmt.Println()
}

// ─────────────────────────────────────────────────────────────
// 2. Iterating Over All Features
// ─────────────────────────────────────────────────────────────
func demoSelectAll(path string) {
	fmt.Println("=== 2. Select All Features ===")

	reader, err := fcb.Open(path)
	if err != nil {
		log.Fatalf("Failed to open: %v", err)
	}
	// Don't defer reader.Close() — SelectAll consumes it

	iter, err := reader.SelectAll()
	if err != nil {
		log.Fatalf("Failed to select all: %v", err)
	}
	defer iter.Close()

	// FeaturesCount() returns the total count from the header
	fmt.Printf("Features available: %d\n", iter.FeaturesCount())

	// Standard Go iteration pattern: Next() + Feature()
	count := 0
	for iter.Next() {
		feature, err := iter.Feature()
		if err != nil {
			log.Fatalf("Failed to get feature %d: %v", count, err)
		}

		// CityFeature has ID and JSON fields
		fmt.Printf("  [%d] ID: %s (JSON: %d bytes)\n",
			count, feature.ID, len(feature.JSON))

		count++
		if count >= 5 {
			break
		}
	}

	// Always check Err() after the iteration loop
	if err := iter.Err(); err != nil {
		log.Fatalf("Iteration error: %v", err)
	}

	fmt.Printf("Read %d of %d features\n\n", count, iter.FeaturesCount())
}

// ─────────────────────────────────────────────────────────────
// 3. Spatial Query: Bounding Box
// ─────────────────────────────────────────────────────────────
func demoSelectBBox(path string) {
	fmt.Println("=== 3. Spatial Query: BBox ===")

	reader, err := fcb.Open(path)
	if err != nil {
		log.Fatalf("Failed to open: %v", err)
	}

	if !reader.HasSpatialIndex() {
		fmt.Println("No spatial index — skipping bbox demo")
		reader.Close()
		return
	}

	// BBox coordinates are in the CRS of the FCB file
	// For the Delft dataset, this is Dutch RD (EPSG:7415)
	bbox := fcb.BBox{
		MinX: 84400.0,
		MinY: 447200.0,
		MaxX: 84600.0,
		MaxY: 447400.0,
	}

	// SelectBBox consumes the reader
	iter, err := reader.SelectBBox(bbox)
	if err != nil {
		log.Fatalf("Failed to select bbox: %v", err)
	}
	defer iter.Close()

	count := 0
	for iter.Next() {
		feature, err := iter.Feature()
		if err != nil {
			log.Fatalf("Failed to get feature: %v", err)
		}
		if count < 3 {
			fmt.Printf("  [%d] %s\n", count, feature.ID)
		}
		count++
	}
	if err := iter.Err(); err != nil {
		log.Fatalf("Iteration error: %v", err)
	}

	fmt.Printf("BBox query returned %d features\n\n", count)
}

// ─────────────────────────────────────────────────────────────
// 4. Working with Feature JSON
// ─────────────────────────────────────────────────────────────
func demoFeatureJSON(path string) {
	fmt.Println("=== 4. Feature JSON Structure ===")

	reader, err := fcb.Open(path)
	if err != nil {
		log.Fatalf("Failed to open: %v", err)
	}

	iter, err := reader.SelectAll()
	if err != nil {
		log.Fatalf("Failed to select all: %v", err)
	}
	defer iter.Close()

	if !iter.Next() {
		fmt.Println("No features found")
		return
	}

	feature, err := iter.Feature()
	if err != nil {
		log.Fatalf("Failed to get feature: %v", err)
	}

	// Parse the CityJSONFeature JSON
	var cjFeature map[string]interface{}
	if err := json.Unmarshal([]byte(feature.JSON), &cjFeature); err != nil {
		log.Fatalf("Invalid JSON: %v", err)
	}

	// Standard CityJSONFeature fields
	fmt.Printf("Feature ID: %s\n", feature.ID)
	fmt.Printf("Type: %v\n", cjFeature["type"])

	// Top-level keys
	keys := make([]string, 0)
	for k := range cjFeature {
		keys = append(keys, k)
	}
	fmt.Printf("Top-level keys: [%s]\n", strings.Join(keys, ", "))

	// CityObjects contain the semantic city model data
	if cityObjects, ok := cjFeature["CityObjects"].(map[string]interface{}); ok {
		fmt.Printf("CityObjects count: %d\n", len(cityObjects))
		for id, obj := range cityObjects {
			if co, ok := obj.(map[string]interface{}); ok {
				fmt.Printf("  '%s': type=%v", id, co["type"])
				if attrs, ok := co["attributes"].(map[string]interface{}); ok {
					fmt.Printf(", attributes=%d", len(attrs))
				}
				fmt.Println()
			}
			break // only show first
		}
	}

	// Vertices array
	if vertices, ok := cjFeature["vertices"].([]interface{}); ok {
		fmt.Printf("Vertices: %d points\n", len(vertices))
	}

	fmt.Println()
}

// ─────────────────────────────────────────────────────────────
// 5. Ownership Model Demo
// ─────────────────────────────────────────────────────────────
func demoOwnershipModel(path string) {
	fmt.Println("=== 5. Ownership Model ===")

	// Demonstrate the consume-on-select pattern:
	reader, err := fcb.Open(path)
	if err != nil {
		log.Fatalf("Failed to open: %v", err)
	}

	// At this point, reader is valid
	fmt.Printf("Before SelectAll: FeaturesCount = %d\n", reader.FeaturesCount())

	iter, err := reader.SelectAll()
	if err != nil {
		log.Fatalf("Failed to select all: %v", err)
	}

	// After SelectAll, reader is consumed (pointer set to nil internally).
	// Calling methods on it returns zero values safely:
	fmt.Printf("After SelectAll: FeaturesCount = %d (reader consumed)\n", reader.FeaturesCount())

	// Close() is safe to call on a consumed reader (no-op):
	reader.Close()

	// The iterator owns the resources now:
	fmt.Printf("Iterator FeaturesCount: %d\n", iter.FeaturesCount())

	// Don't forget to close the iterator when done:
	iter.Close()

	// After closing, the iterator returns safe defaults:
	fmt.Printf("After iter.Close(): FeaturesCount = %d\n", iter.FeaturesCount())
	fmt.Printf("After iter.Close(): Next() = %v\n\n", iter.Next())
}

// ─────────────────────────────────────────────────────────────
// 6. Error Handling
// ─────────────────────────────────────────────────────────────
func demoErrorHandling() {
	fmt.Println("=== 6. Error Handling ===")

	// Invalid file path
	_, err := fcb.Open("/nonexistent/path.fcb")
	if err != nil {
		fmt.Printf("Invalid path error: %v\n", err)
	}

	// Accessing a closed reader
	reader, err := fcb.Open(os.Args[1])
	if err != nil {
		log.Fatalf("Failed to open: %v", err)
	}
	reader.Close()
	// After Close, methods return zero values safely
	fmt.Printf("Closed reader FeaturesCount: %d\n", reader.FeaturesCount())
	fmt.Printf("Closed reader HasSpatialIndex: %v\n", reader.HasSpatialIndex())

	// Attempting to use a consumed reader
	reader2, _ := fcb.Open(os.Args[1])
	iter, _ := reader2.SelectAll()
	_, err = reader2.SelectAll() // reader2 is consumed
	if err != nil {
		fmt.Printf("Consumed reader error: %v\n", err)
	}
	iter.Close()
}
