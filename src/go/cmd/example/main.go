// Example demonstrates the FlatCityBuf Go bindings API.
//
// Build the Rust static library first:
//
//	just build-go-lib
//
// Then run:
//
//	cd src/go && go run cmd/example/main.go ../../examples/data/delft.fcb
package main

import (
	"encoding/json"
	"fmt"
	"log"
	"os"

	"github.com/cityjson/flatcitybuf-go/fcb"
)

func main() {
	if len(os.Args) < 2 {
		fmt.Fprintf(os.Stderr, "Usage: %s <path-to-fcb-file>\n", os.Args[0])
		os.Exit(1)
	}
	path := os.Args[1]

	// ─── Open FCB File ──────────────────────────────────────
	fmt.Println("=== Opening FCB File ===")
	reader, err := fcb.Open(path)
	if err != nil {
		log.Fatalf("Failed to open: %v", err)
	}
	defer reader.Close()

	fmt.Printf("Feature count: %d\n", reader.FeaturesCount())
	fmt.Printf("Has spatial index: %v\n", reader.HasSpatialIndex())

	// ─── CityJSON Metadata ──────────────────────────────────
	fmt.Println("\n=== CityJSON Metadata ===")
	meta, err := reader.CityJSONMetadata()
	if err != nil {
		log.Fatalf("Failed to get metadata: %v", err)
	}

	fmt.Printf("Type: %v\n", meta["type"])
	fmt.Printf("Version: %v\n", meta["version"])

	if transform, ok := meta["transform"].(map[string]interface{}); ok {
		fmt.Printf("Transform scale: %v\n", transform["scale"])
		fmt.Printf("Transform translate: %v\n", transform["translate"])
	}

	// ─── Select All Features ────────────────────────────────
	fmt.Println("\n=== Select All Features (first 5) ===")
	selectAllExample(path)

	// ─── Spatial Query: BBox ────────────────────────────────
	fmt.Println("\n=== Spatial Query: BBox ===")
	selectBBoxExample(path)

	// ─── Full Feature JSON ──────────────────────────────────
	fmt.Println("\n=== Feature JSON Structure ===")
	featureJsonExample(path)

	fmt.Println("\nDone!")
}

func selectAllExample(path string) {
	reader, err := fcb.Open(path)
	if err != nil {
		log.Fatalf("Failed to open: %v", err)
	}
	// Reader is consumed by SelectAll, no need to defer Close

	iter, err := reader.SelectAll()
	if err != nil {
		log.Fatalf("Failed to select all: %v", err)
	}
	defer iter.Close()

	fmt.Printf("Total features: %d\n", iter.FeaturesCount())

	count := 0
	for iter.Next() {
		feature, err := iter.Feature()
		if err != nil {
			log.Fatalf("Failed to get feature: %v", err)
		}
		fmt.Printf("  [%d] ID: %s (JSON length: %d bytes)\n",
			count, feature.ID, len(feature.JSON))
		count++
		if count >= 5 {
			break
		}
	}
	if err := iter.Err(); err != nil {
		log.Fatalf("Iteration error: %v", err)
	}
	fmt.Printf("Read %d features\n", count)
}

func selectBBoxExample(path string) {
	reader, err := fcb.Open(path)
	if err != nil {
		log.Fatalf("Failed to open: %v", err)
	}

	if !reader.HasSpatialIndex() {
		fmt.Println("File does not have a spatial index, skipping bbox query")
		reader.Close()
		return
	}

	// Bounding box covering part of Delft (Netherlands RD coordinates)
	bbox := fcb.BBox{
		MinX: 84400.0,
		MinY: 447200.0,
		MaxX: 84600.0,
		MaxY: 447400.0,
	}

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
			fmt.Printf("  [%d] ID: %s\n", count, feature.ID)
		}
		count++
	}
	if err := iter.Err(); err != nil {
		log.Fatalf("Iteration error: %v", err)
	}
	fmt.Printf("BBox query returned %d features\n", count)
}

func featureJsonExample(path string) {
	reader, err := fcb.Open(path)
	if err != nil {
		log.Fatalf("Failed to open: %v", err)
	}

	iter, err := reader.SelectAll()
	if err != nil {
		log.Fatalf("Failed to select all: %v", err)
	}
	defer iter.Close()

	if iter.Next() {
		feature, err := iter.Feature()
		if err != nil {
			log.Fatalf("Failed to get feature: %v", err)
		}

		// Parse JSON to inspect structure
		var parsed map[string]interface{}
		if err := json.Unmarshal([]byte(feature.JSON), &parsed); err != nil {
			log.Fatalf("Invalid JSON: %v", err)
		}

		fmt.Printf("Feature ID: %s\n", feature.ID)
		fmt.Printf("JSON top-level keys: ")
		for key := range parsed {
			fmt.Printf("%s ", key)
		}
		fmt.Println()

		// Pretty-print a trimmed version
		if cityObjects, ok := parsed["CityObjects"].(map[string]interface{}); ok {
			fmt.Printf("CityObjects count: %d\n", len(cityObjects))
			for id, obj := range cityObjects {
				if co, ok := obj.(map[string]interface{}); ok {
					fmt.Printf("  CityObject '%s': type=%v\n", id, co["type"])
				}
				break // just show first one
			}
		}

		if vertices, ok := parsed["vertices"].([]interface{}); ok {
			fmt.Printf("Vertices count: %d\n", len(vertices))
		}
	}
}
