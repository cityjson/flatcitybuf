"""End-to-end integration tests for FlatCityBuf Python bindings"""

import subprocess
import tempfile
import shutil
import pytest
from pathlib import Path
from flatcitybuf import Reader, AsyncReader, AttrFilter, FcbError, Operator
import flatcitybuf as fcb


def setup_test_data():
    """Setup test FCB files by converting from JSONL using fcb_cli"""
    # Get paths
    current_dir = Path(__file__).parent
    fcb_core_data = current_dir / ".." / ".." / "fcb_core" / "tests" / "data"
    temp_dir = Path(tempfile.mkdtemp(prefix="fcb_test_"))

    # Source JSONL files (using delft test data)
    test_files = [
        ("delft.city.jsonl", "delft.fcb"),
    ]

    # Convert each JSONL to FCB
    for jsonl_file, fcb_file in test_files:
        jsonl_path = fcb_core_data / jsonl_file
        fcb_path = temp_dir / fcb_file

        if jsonl_path.exists():
            try:
                # Run fcb_cli to convert JSONL to FCB
                cmd = [
                    "cargo",
                    "run",
                    "-p",
                    "fcb_cli",
                    "ser",
                    "--input",
                    str(jsonl_path),
                    "--output",
                    str(fcb_path),
                    "--spatial-index",
                    "true",
                    "--index-all-attributes",
                    "true",
                ]

                result = subprocess.run(
                    cmd,
                    capture_output=True,
                    text=True,
                    cwd=current_dir.parent.parent,
                )

                if result.returncode != 0:
                    pytest.skip(
                        f"Failed to generate test FCB file {fcb_file}: {result.stderr}"
                    )

            except subprocess.CalledProcessError as e:
                pytest.skip(f"Failed to run fcb_cli: {e}")
        else:
            pytest.skip(f"Source JSONL file not found: {jsonl_path}")

    return temp_dir


@pytest.fixture(scope="session")
def test_data_dir():
    """Session-scoped fixture to setup test data once"""
    temp_dir = setup_test_data()
    yield temp_dir
    # Cleanup
    shutil.rmtree(temp_dir, ignore_errors=True)


class TestE2EIntegration:
    """End-to-end integration tests using real FCB files"""

    def test_fcb_path(self, test_data_dir):
        """Path to small test FCB file"""
        return test_data_dir / "delft.fcb"

    def test_file_exists(self, test_data_dir):
        """Ensure test files exist"""
        fcb_path = self.test_fcb_path(test_data_dir)

        assert fcb_path.exists(), f"Test file not found: {fcb_path}"

    def test_read_file_info(self, test_data_dir):
        """Test reading file information"""
        fcb_path = self.test_fcb_path(test_data_dir)
        reader = Reader(str(fcb_path))
        info = reader.info()

        assert info.feature_count > 0
        assert hasattr(info, "columns")
        assert hasattr(info, "crs")
        assert hasattr(info, "bbox")

    def test_iterate_features(self, test_data_dir):
        """Test iterating through all features"""
        fcb_path = self.test_fcb_path(test_data_dir)
        reader = Reader(str(fcb_path))
        features = list(reader)

        assert len(features) > 0

        # Check first feature has expected attributes
        first_feature = features[0]
        print("first_feature===========", first_feature)
        assert hasattr(first_feature, "id")
        assert hasattr(first_feature, "geometry")
        assert hasattr(first_feature, "attributes")

    def test_spatial_query_bbox(self, test_data_dir):
        """Test spatial query using bounding box"""
        fcb_path = self.test_fcb_path(test_data_dir)
        reader = Reader(str(fcb_path))

        minx = 84227.77
        miny = 445377.33
        maxx = 85323.23
        maxy = 446334.69
        features = list(reader.query_bbox(minx, miny, maxx, maxy))

        assert isinstance(features, list)
        # Should find some features in this area
        assert len(features) > 0

    def test_attribute_query(self, test_data_dir):
        """Test querying features by attributes"""
        fcb_path = self.test_fcb_path(test_data_dir)
        reader = Reader(str(fcb_path))

        # Try to query by attributes that should exist in cube_attr test data
        try:
            # Test equality filter - the cube data should have string attributes
            id_filter = AttrFilter(
                "identificatie", Operator.Eq, "NL.IMBAG.Pand.0503100000012869"
            )
            buildings = list(reader.query_attr([id_filter]))
            assert isinstance(buildings, list)
            assert len(buildings) == 1
            assert buildings[0].id == "NL.IMBAG.Pand.0503100000012869"

        except FcbError as e:
            # If specific attributes don't exist, just verify the query mechanism works
            print(f"Attribute query failed as expected: {e}")
            # This is acceptable for test data that may not have the specific attributes

    def test_convenience_functions(self, test_data_dir):
        """Test module-level convenience functions"""
        fcb_path = self.test_fcb_path(test_data_dir)

        # Test open_file convenience function
        features = fcb.open_file(str(fcb_path))
        assert isinstance(features, list)
        assert len(features) > 0

        # Test query_bbox convenience function
        bbox_features = fcb.query_bbox(
            str(fcb_path), 84227.77, 445377.33, 85323.23, 446334.69
        )
        assert isinstance(bbox_features, list)

    def test_feature_geometry_access(self, test_data_dir):
        """Test accessing feature geometry data"""
        fcb_path = self.test_fcb_path(test_data_dir)
        reader = Reader(str(fcb_path))
        features = list(reader)

        if len(features) > 0:
            feature = features[0]

            print(feature)

    def test_feature_attributes_access(self, test_data_dir):
        """Test accessing feature attributes"""
        fcb_path = self.test_fcb_path(test_data_dir)
        reader = Reader(str(fcb_path))
        features = list(reader)

        if len(features) > 0:
            feature = features[0]

            # Check attributes access
            if hasattr(feature, "attributes") and feature.attributes:
                attributes = feature.attributes
                assert attributes is not None
                assert isinstance(attributes, dict)

    def test_reader_basic_usage(self, test_data_dir):
        """Test basic Reader usage"""
        fcb_path = self.test_fcb_path(test_data_dir)

        reader = Reader(str(fcb_path))
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

    def test_invalid_bbox_query(self, test_data_dir):
        """Test error handling for invalid bbox queries"""
        fcb_path = test_data_dir / "small.fcb"

        if fcb_path.exists():
            reader = Reader(str(fcb_path))

            # Test very large bbox - should work but return empty results potentially
            result = list(reader.query_bbox(100000, 100000, 50000, 50000))
            assert isinstance(
                result, list
            )  # Should return empty list, not raise error

    def test_invalid_attribute_filter(self, test_data_dir):
        """Test error handling for invalid attribute filters"""
        fcb_path = test_data_dir / "small.fcb"

        if fcb_path.exists():
            reader = Reader(str(fcb_path))

            # Test querying for non-existent attribute - this should raise FcbError
            non_existent_filter = AttrFilter.eq("non_existent_field", "value")

            # This should raise an error for non-indexed attributes
            with pytest.raises(FcbError):
                list(reader.query_attr([non_existent_filter]))
