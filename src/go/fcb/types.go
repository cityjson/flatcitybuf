package fcb

// BBox represents a 2D bounding box for spatial queries.
type BBox struct {
	MinX float64
	MinY float64
	MaxX float64
	MaxY float64
}

// CityFeature represents a single CityJSON feature.
type CityFeature struct {
	// ID is the feature identifier.
	ID string
	// JSON is the full CityJSONFeature serialized as a JSON string.
	JSON string
}
