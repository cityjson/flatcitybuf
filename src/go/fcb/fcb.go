// Package fcb provides Go bindings for reading FlatCityBuf files.
//
// FlatCityBuf (FCB) is a binary format for CityJSON data that supports
// spatial and attribute indexing for efficient queries.
//
// # Usage
//
//	reader, err := fcb.Open("path/to/file.fcb")
//	if err != nil {
//	    log.Fatal(err)
//	}
//	defer reader.Close()
//
//	iter, err := reader.SelectAll()
//	if err != nil {
//	    log.Fatal(err)
//	}
//	defer iter.Close()
//
//	for iter.Next() {
//	    feature, err := iter.Feature()
//	    if err != nil {
//	        log.Fatal(err)
//	    }
//	    fmt.Println(feature.ID, feature.JSON)
//	}
package fcb

/*
#cgo CFLAGS: -I${SRCDIR}/../include
#cgo LDFLAGS: -L${SRCDIR}/../../rust/target/release -lfcb_go -lm -ldl -lpthread
#include "fcb_core.h"
#include <stdlib.h>
*/
import "C"

import (
	"encoding/json"
	"fmt"
	"unsafe"
)

// Reader represents an open FCB file for reading.
type Reader struct {
	ptr *C.struct_FcbFileReader
}

// Open opens an FCB file at the given path for reading.
func Open(path string) (*Reader, error) {
	cpath := C.CString(path)
	defer C.free(unsafe.Pointer(cpath))

	var errPtr *C.char
	ptr := C.fcb_reader_open(cpath, &errPtr)
	if ptr == nil {
		return nil, extractError(errPtr)
	}

	return &Reader{ptr: ptr}, nil
}

// FeaturesCount returns the total number of features in the file.
func (r *Reader) FeaturesCount() uint64 {
	if r.ptr == nil {
		return 0
	}
	return uint64(C.fcb_reader_features_count(r.ptr))
}

// HasSpatialIndex returns true if the file has a spatial index.
func (r *Reader) HasSpatialIndex() bool {
	if r.ptr == nil {
		return false
	}
	return bool(C.fcb_reader_has_spatial_index(r.ptr))
}

// CityJSONMetadata returns the CityJSON metadata as a parsed JSON object.
func (r *Reader) CityJSONMetadata() (map[string]interface{}, error) {
	if r.ptr == nil {
		return nil, fmt.Errorf("reader is closed")
	}

	var errPtr *C.char
	cjson := C.fcb_reader_cityjson_metadata(r.ptr, &errPtr)
	if cjson == nil {
		return nil, extractError(errPtr)
	}
	defer C.fcb_free_string(cjson)

	jsonStr := C.GoString(cjson)
	var result map[string]interface{}
	if err := json.Unmarshal([]byte(jsonStr), &result); err != nil {
		return nil, fmt.Errorf("failed to parse metadata: %w", err)
	}
	return result, nil
}

// SelectAll selects all features for iteration. Consumes the reader.
// After calling SelectAll, the reader should not be used again (Close becomes a no-op).
func (r *Reader) SelectAll() (*FeatureIter, error) {
	if r.ptr == nil {
		return nil, fmt.Errorf("reader is closed")
	}

	var errPtr *C.char
	ptr := C.fcb_reader_select_all(r.ptr, &errPtr)
	r.ptr = nil // Reader is consumed
	if ptr == nil {
		return nil, extractError(errPtr)
	}

	return &FeatureIter{ptr: ptr}, nil
}

// SelectBBox selects features within a bounding box. Consumes the reader.
func (r *Reader) SelectBBox(bbox BBox) (*FeatureIter, error) {
	if r.ptr == nil {
		return nil, fmt.Errorf("reader is closed")
	}

	var errPtr *C.char
	ptr := C.fcb_reader_select_bbox(
		r.ptr,
		C.double(bbox.MinX),
		C.double(bbox.MinY),
		C.double(bbox.MaxX),
		C.double(bbox.MaxY),
		&errPtr,
	)
	r.ptr = nil // Reader is consumed
	if ptr == nil {
		return nil, extractError(errPtr)
	}

	return &FeatureIter{ptr: ptr}, nil
}

// Close frees the reader. Safe to call multiple times.
func (r *Reader) Close() {
	if r.ptr != nil {
		C.fcb_reader_free(r.ptr)
		r.ptr = nil
	}
}

// FeatureIter iterates over selected features.
type FeatureIter struct {
	ptr     *C.struct_FcbFileIterator
	hasNext bool
	err     error
}

// Next advances to the next feature. Returns true if a feature is available.
// After Next returns false, check Err() for any errors.
func (it *FeatureIter) Next() bool {
	if it.ptr == nil {
		return false
	}

	var errPtr *C.char
	result := C.fcb_iterator_next(it.ptr, &errPtr)
	switch result {
	case 1:
		it.hasNext = true
		return true
	case 0:
		it.hasNext = false
		return false
	default: // -1
		it.hasNext = false
		it.err = extractError(errPtr)
		return false
	}
}

// Feature returns the current feature. Call after Next() returns true.
func (it *FeatureIter) Feature() (*CityFeature, error) {
	if it.ptr == nil || !it.hasNext {
		return nil, fmt.Errorf("no current feature - call Next() first")
	}

	var errPtr *C.char

	cjson := C.fcb_iterator_current_json(it.ptr, &errPtr)
	if cjson == nil {
		return nil, extractError(errPtr)
	}
	defer C.fcb_free_string(cjson)

	cid := C.fcb_iterator_current_id(it.ptr, &errPtr)
	if cid == nil {
		return nil, extractError(errPtr)
	}
	defer C.fcb_free_string(cid)

	return &CityFeature{
		ID:   C.GoString(cid),
		JSON: C.GoString(cjson),
	}, nil
}

// Err returns any error encountered during iteration.
func (it *FeatureIter) Err() error {
	return it.err
}

// FeaturesCount returns the number of selected features.
func (it *FeatureIter) FeaturesCount() uint64 {
	if it.ptr == nil {
		return 0
	}
	return uint64(C.fcb_iterator_features_count(it.ptr))
}

// Close frees the iterator. Safe to call multiple times.
func (it *FeatureIter) Close() {
	if it.ptr != nil {
		C.fcb_iterator_free(it.ptr)
		it.ptr = nil
	}
}

// extractError converts a C error string to a Go error and frees the C string.
func extractError(errPtr *C.char) error {
	if errPtr == nil {
		return fmt.Errorf("unknown error")
	}
	msg := C.GoString(errPtr)
	C.fcb_free_string(errPtr)
	return fmt.Errorf("%s", msg)
}
