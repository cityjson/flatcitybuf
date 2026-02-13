package fcb

import (
	"encoding/json"
	"path/filepath"
	"runtime"
	"testing"
)

func testDataPath() string {
	_, filename, _, _ := runtime.Caller(0)
	return filepath.Join(filepath.Dir(filename), "..", "..", "..", "examples", "data", "delft.fcb")
}

func TestOpen(t *testing.T) {
	path := testDataPath()
	reader, err := Open(path)
	if err != nil {
		t.Fatalf("Failed to open FCB file: %v", err)
	}
	defer reader.Close()

	count := reader.FeaturesCount()
	if count == 0 {
		t.Error("Expected non-zero feature count")
	}
	t.Logf("Feature count: %d", count)
}

func TestOpenInvalidPath(t *testing.T) {
	_, err := Open("/nonexistent/path.fcb")
	if err == nil {
		t.Error("Expected error for invalid path")
	}
}

func TestHasSpatialIndex(t *testing.T) {
	reader, err := Open(testDataPath())
	if err != nil {
		t.Fatalf("Failed to open: %v", err)
	}
	defer reader.Close()

	hasSpatial := reader.HasSpatialIndex()
	t.Logf("Has spatial index: %v", hasSpatial)
}

func TestCityJSONMetadata(t *testing.T) {
	reader, err := Open(testDataPath())
	if err != nil {
		t.Fatalf("Failed to open: %v", err)
	}
	defer reader.Close()

	meta, err := reader.CityJSONMetadata()
	if err != nil {
		t.Fatalf("Failed to get metadata: %v", err)
	}

	if meta == nil {
		t.Fatal("Expected non-nil metadata")
	}

	// Check that common CityJSON fields exist
	if _, ok := meta["type"]; !ok {
		t.Error("Expected 'type' field in metadata")
	}
	t.Logf("Metadata keys: %v", keys(meta))
}

func TestSelectAll(t *testing.T) {
	reader, err := Open(testDataPath())
	if err != nil {
		t.Fatalf("Failed to open: %v", err)
	}
	// Don't defer Close - reader is consumed by SelectAll

	iter, err := reader.SelectAll()
	if err != nil {
		t.Fatalf("Failed to select all: %v", err)
	}
	defer iter.Close()

	totalCount := iter.FeaturesCount()
	t.Logf("Total features: %d", totalCount)

	count := 0
	for iter.Next() {
		feature, err := iter.Feature()
		if err != nil {
			t.Fatalf("Failed to get feature %d: %v", count, err)
		}
		if feature.ID == "" {
			t.Errorf("Feature %d has empty ID", count)
		}
		if feature.JSON == "" {
			t.Errorf("Feature %d has empty JSON", count)
		}
		// Verify JSON is valid
		var parsed map[string]interface{}
		if err := json.Unmarshal([]byte(feature.JSON), &parsed); err != nil {
			t.Errorf("Feature %d has invalid JSON: %v", count, err)
		}
		count++
		if count >= 5 {
			break // Just test first 5 features
		}
	}

	if err := iter.Err(); err != nil {
		t.Fatalf("Iteration error: %v", err)
	}

	if count == 0 {
		t.Error("Expected at least one feature")
	}
	t.Logf("Read %d features successfully", count)
}

func TestSelectBBox(t *testing.T) {
	reader, err := Open(testDataPath())
	if err != nil {
		t.Fatalf("Failed to open: %v", err)
	}

	if !reader.HasSpatialIndex() {
		t.Skip("File has no spatial index")
	}

	// Use a bbox that covers part of Delft (Netherlands)
	bbox := BBox{
		MinX: 84400.0,
		MinY: 447200.0,
		MaxX: 84600.0,
		MaxY: 447400.0,
	}

	iter, err := reader.SelectBBox(bbox)
	if err != nil {
		t.Fatalf("Failed to select bbox: %v", err)
	}
	defer iter.Close()

	count := 0
	for iter.Next() {
		_, err := iter.Feature()
		if err != nil {
			t.Fatalf("Failed to get feature: %v", err)
		}
		count++
	}

	if err := iter.Err(); err != nil {
		t.Fatalf("Iteration error: %v", err)
	}

	t.Logf("BBox query returned %d features", count)
}

func keys(m map[string]interface{}) []string {
	k := make([]string, 0, len(m))
	for key := range m {
		k = append(k, key)
	}
	return k
}
