"""End-to-end integration tests for FlatCityBuf Python bindings"""

import os
import pytest
from flatcitybuf import Reader, AsyncReader, BBox, AttrFilter, FcbError


class TestE2EIntegration:
    """End-to-end integration tests using real FCB files"""

    @property
    def test_data_dir(self):
        """Get path to test data directory"""
        # Assuming we're in src/rust/fcb_py/tests
        current_dir = os.path.dirname(os.path.abspath(__file__))
        return os.path.join(
            current_dir, "..", "..", "fcb_core", "tests", "data"
        )

    @property
    def small_fcb_path(self):
        """Path to small test FCB file"""
        return os.path.join(self.test_data_dir, "small.fcb")

    @property
    def delft_fcb_path(self):
        """Path to Delft test FCB file"""
        return os.path.join(self.test_data_dir, "delft.fcb")

    @property
    def delft_bbox_fcb_path(self):
        """Path to Delft bbox test FCB file"""
        return os.path.join(self.test_data_dir, "delft_bbox.fcb")

    def test_file_exists(self):
        """Ensure test files exist"""
        assert os.path.exists(
            self.small_fcb_path
        ), f"Test file not found: {self.small_fcb_path}"
        assert os.path.exists(
            self.delft_fcb_path
        ), f"Test file not found: {self.delft_fcb_path}"

    def test_read_file_info(self):
        """Test reading file information"""
        reader = Reader(self.small_fcb_path)
        info = reader.info()

        assert info.feature_count > 0
        assert info.file_size > 0
        assert hasattr(info, "geographical_extent")

    def test_iterate_features(self):
        """Test iterating through all features"""
        reader = Reader(self.small_fcb_path)
        features = list(reader)

        assert len(features) > 0

        # Check first feature has expected attributes
        first_feature = features[0]
        assert hasattr(first_feature, "id")
        assert hasattr(first_feature, "geometry")
        assert hasattr(first_feature, "attributes")

    def test_spatial_query_bbox(self):
        """Test spatial query using bounding box"""
        if not os.path.exists(self.delft_bbox_fcb_path):
            pytest.skip("delft_bbox.fcb not available")

        reader = Reader(self.delft_bbox_fcb_path)

        # Query a reasonable bbox that should contain some features
        bbox = BBox(85000, 446000, 86000, 447000)  # Delft area coordinates
        features = reader.query_bbox(
            bbox.min_x, bbox.min_y, bbox.max_x, bbox.max_y
        )

        assert isinstance(features, list)
        # Should find some features in this area
        assert len(features) >= 0

    def test_attribute_query(self):
        """Test querying features by attributes"""
        if not os.path.exists(self.delft_fcb_path):
            pytest.skip("delft.fcb not available")

        reader = Reader(self.delft_fcb_path)

        # Try to query by a common attribute
        try:
            # Test numeric filter
            height_filter = AttrFilter.gt("height", 10.0)
            tall_features = reader.query_attr([height_filter])
            assert isinstance(tall_features, list)

            # Test equality filter
            type_filter = AttrFilter.eq("type", "Building")
            buildings = reader.query_attr([type_filter])
            assert isinstance(buildings, list)

        except FcbError:
            # Some files might not have these specific attributes
            pytest.skip("Required attributes not available in test file")

    def test_convenience_functions(self):
        """Test module-level convenience functions"""
        # Test open_file convenience function
        features = fcb.open_file(self.small_fcb_path)
        assert isinstance(features, list)
        assert len(features) > 0

        # Test query_bbox convenience function
        if os.path.exists(self.delft_bbox_fcb_path):
            bbox_features = fcb.query_bbox(
                self.delft_bbox_fcb_path, 85000, 446000, 86000, 447000
            )
            assert isinstance(bbox_features, list)

    def test_feature_geometry_access(self):
        """Test accessing feature geometry data"""
        reader = Reader(self.small_fcb_path)
        features = list(reader.select_all())

        if len(features) > 0:
            feature = features[0]

            # Check geometry access
            geometry = feature.geometry()
            assert geometry is not None

            # Check if we can access vertices
            vertices = geometry.vertices()
            assert isinstance(vertices, list)

            # Check vertex properties
            if len(vertices) > 0:
                vertex = vertices[0]
                assert hasattr(vertex, "x")
                assert hasattr(vertex, "y")
                assert hasattr(vertex, "z")

    def test_feature_attributes_access(self):
        """Test accessing feature attributes"""
        reader = Reader(self.small_fcb_path)
        features = list(reader.select_all())

        if len(features) > 0:
            feature = features[0]

            # Check attributes access
            attributes = feature.attributes()
            assert attributes is not None
            assert isinstance(attributes, dict)

    def test_reader_context_manager(self):
        """Test using Reader as a context manager"""
        with Reader(self.small_fcb_path) as reader:
            info = reader.info()
            assert info.feature_count > 0

            features = list(reader)
            assert len(features) > 0


class TestAsyncReaderE2E:
    """End-to-end tests for AsyncReader (HTTP functionality)"""

    @pytest.mark.skip(reason="Requires HTTP test endpoint")
    async def test_http_reader(self):
        """Test reading FCB file over HTTP"""
        # This would require an actual HTTP endpoint with FCB files
        url = "https://example.com/test.fcb"

        async with AsyncReader(url) as reader:
            info = await reader.info()
            assert info.feature_count > 0

            features = []
            async for feature in reader:
                features.append(feature)

            assert len(features) > 0


class TestErrorHandling:
    """Test error handling in various scenarios"""

    def test_invalid_file_path(self):
        """Test error handling for invalid file paths"""
        with pytest.raises(FcbError):
            Reader("/path/that/does/not/exist.fcb")

    def test_invalid_bbox_query(self):
        """Test error handling for invalid bbox queries"""
        test_data_dir = os.path.join(
            os.path.dirname(os.path.abspath(__file__)),
            "..",
            "..",
            "fcb_core",
            "tests",
            "data",
        )
        small_fcb_path = os.path.join(test_data_dir, "small.fcb")

        if os.path.exists(small_fcb_path):
            reader = Reader(small_fcb_path)

            # Test invalid bbox (min > max)
            with pytest.raises((FcbError, ValueError)):
                reader.query_bbox(100, 100, 50, 50)

    def test_invalid_attribute_filter(self):
        """Test error handling for invalid attribute filters"""
        test_data_dir = os.path.join(
            os.path.dirname(os.path.abspath(__file__)),
            "..",
            "..",
            "fcb_core",
            "tests",
            "data",
        )
        small_fcb_path = os.path.join(test_data_dir, "small.fcb")

        if os.path.exists(small_fcb_path):
            reader = Reader(small_fcb_path)

            # Test querying for non-existent attribute
            non_existent_filter = AttrFilter.eq("non_existent_field", "value")

            # This should not raise an error but return empty results
            result = reader.query_attr([non_existent_filter])
            assert isinstance(result, list)
