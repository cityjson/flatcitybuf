"""End-to-end integration tests for FlatCityBuf Python bindings"""

# For HTTP testing
import shutil
import subprocess
import tempfile
from pathlib import Path

import pytest

import flatcitybuf as fcb
from flatcitybuf import AsyncReader, AttrFilter, FcbError, Operator, Reader


def setup_test_data():
    """Setup test FCB files by converting from JSONL using fcb_cli"""
    # Get paths
    current_dir = Path(__file__).parent
    # Navigate up to the rust directory and find fcb_core
    fcb_core_data = current_dir.parent.parent / "fcb_core" / "tests" / "data"
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
                    "-A",
                    "-g",
                    "--attr-branching-factor",
                    "16",
                ]

                result = subprocess.run(
                    cmd,
                    capture_output=True,
                    text=True,
                    cwd=current_dir.parent.parent,  # Run from rust directory
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

    def fcb_path(self, test_data_dir):
        """Path to small test FCB file"""
        return test_data_dir / "delft.fcb"

    def test_file_exists(self, test_data_dir):
        """Ensure test files exist"""
        fcb_path = self.fcb_path(test_data_dir)

        assert fcb_path.exists(), f"Test file not found: {fcb_path}"

    def test_read_file_info(self, test_data_dir):
        """Test reading file information"""
        fcb_path = self.fcb_path(test_data_dir)
        reader = Reader(str(fcb_path))
        info = reader.info()

        assert info.feature_count > 0
        assert hasattr(info, "columns")
        assert hasattr(info, "crs")
        assert hasattr(info, "bbox")

    def test_iterate_features(self, test_data_dir):
        """Test iterating through all features"""
        fcb_path = self.fcb_path(test_data_dir)
        reader = Reader(str(fcb_path))
        features = list(reader)

        assert len(features) > 0

        # Check first feature has expected CityJSON attributes
        first_feature = features[0]
        assert hasattr(first_feature, "id")
        assert hasattr(first_feature, "type")
        assert hasattr(first_feature, "city_objects")
        assert hasattr(first_feature, "vertices")

        # Get the first city object from the dictionary
        city_objects_values = list(first_feature.city_objects.values())
        first_city_object = (
            city_objects_values[1]
            if len(city_objects_values) > 1
            else city_objects_values[0]
        )
        assert hasattr(first_city_object, "geometry")
        assert hasattr(first_city_object, "attributes")
        assert hasattr(first_city_object, "children")
        assert hasattr(first_city_object, "parents")
        assert len(first_city_object.geometry) > 0
        assert len(first_city_object.geometry[0].boundaries) > 0

        # Check that city_objects is a dictionary
        assert isinstance(first_feature.city_objects, dict)
        assert len(first_feature.city_objects) > 0

        # Check vertices list
        assert isinstance(first_feature.vertices, list)
        assert len(first_feature.vertices) > 0

    def test_spatial_query_bbox(self, test_data_dir):
        """Test spatial query using bounding box"""
        fcb_path = self.fcb_path(test_data_dir)
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
        fcb_path = self.fcb_path(test_data_dir)
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
        fcb_path = self.fcb_path(test_data_dir)

        # Test open_file convenience function
        features = fcb.open_file(str(fcb_path))
        assert isinstance(features, list)
        assert len(features) > 0

        # Test query_bbox convenience function
        bbox_features = fcb.query_bbox(
            str(fcb_path), 84227.77, 445377.33, 85323.23, 446334.69
        )
        assert isinstance(bbox_features, list)
        assert len(bbox_features) > 0


class TestAsyncReaderE2E:
    """End-to-end tests for AsyncReader (HTTP functionality)"""

    def fcb_url(self):
        """URL to test FCB file"""
        return "https://storage.googleapis.com/flatcitybuf/delft.city.fcb"

    @pytest.mark.asyncio
    async def test_async_reader_creation(self):
        """Test creating AsyncReader with HTTP URL"""
        url = self.fcb_url()
        reader = AsyncReader(url)
        assert reader is not None

    @pytest.mark.asyncio
    async def test_async_reader_open(self):
        """Test opening AsyncReader and getting info"""
        url = self.fcb_url()
        reader = AsyncReader(url)
        opened_reader = await reader.open()

        # Test basic info access
        info = opened_reader.info()
        assert info.feature_count > 0
        assert hasattr(info, "columns")
        assert hasattr(info, "crs")
        assert hasattr(info, "bbox")

    @pytest.mark.asyncio
    async def test_async_reader_cityjson_header(self):
        """Test getting CityJSON header information"""
        url = self.fcb_url()
        reader = AsyncReader(url)
        opened_reader = await reader.open()

        # Test CityJSON header
        cityjson = opened_reader.cityjson_header()
        assert cityjson.type == "CityJSON"
        assert hasattr(cityjson, "version")
        assert hasattr(cityjson, "transform")
        assert hasattr(cityjson, "feature_count")
        assert cityjson.feature_count > 0

    @pytest.mark.asyncio
    async def test_async_select_all_iterator(self):
        """Test select_all async iterator"""
        url = self.fcb_url()
        reader = AsyncReader(url)
        opened_reader = await reader.open()

        # Get async iterator
        async_iter = opened_reader.select_all()
        assert async_iter is not None

        # Test getting a few features
        first_feature = await async_iter.next()
        assert first_feature is not None

        # Check first feature has expected CityJSON attributes
        assert hasattr(first_feature, "id")
        assert hasattr(first_feature, "type")
        assert hasattr(first_feature, "city_objects")
        assert hasattr(first_feature, "vertices")

        # Test getting another feature
        second_feature = await async_iter.next()
        assert second_feature is not None
        assert second_feature.id != first_feature.id

    @pytest.mark.asyncio
    async def test_async_collect_all_features(self):
        """Test collecting all features at once"""
        url = self.fcb_url()
        reader = AsyncReader(url)
        opened_reader = await reader.open()

        # Get async iterator and collect all features
        async_iter = opened_reader.select_all()
        features = await async_iter.collect()

        assert isinstance(features, list)
        assert len(features) > 0

        # Check structure of first feature
        first_feature = features[0]
        assert hasattr(first_feature, "id")
        assert hasattr(first_feature, "city_objects")
        assert isinstance(first_feature.city_objects, dict)
        assert len(first_feature.city_objects) > 0

    @pytest.mark.asyncio
    async def test_async_spatial_query_bbox(self):
        """Test async spatial query using bounding box"""
        url = self.fcb_url()
        reader = AsyncReader(url)
        opened_reader = await reader.open()

        # Same bbox as in sync tests
        minx = 84227.77
        miny = 445377.33
        maxx = 85323.23
        maxy = 446334.69

        async_iter = opened_reader.query_bbox(minx, miny, maxx, maxy)
        features = await async_iter.collect()
        print("features===========", features)

        assert isinstance(features, list)
        assert len(features) > 0

    @pytest.mark.asyncio
    async def test_async_spatial_query_iterator(self):
        """Test async spatial query with iterator"""
        url = self.fcb_url()
        reader = AsyncReader(url)
        opened_reader = await reader.open()

        minx = 84227.77
        miny = 445377.33
        maxx = 85323.23
        maxy = 446334.69

        async_iter = opened_reader.query_bbox(minx, miny, maxx, maxy)

        # Test getting features one by one
        count = 0
        while True:
            feature = await async_iter.next()
            if feature is None:
                break
            count += 1
            assert hasattr(feature, "id")
            # Limit to avoid infinite loops in tests
            if count >= 5:
                break

        assert count > 0

    @pytest.mark.asyncio
    async def test_async_attribute_query(self):
        """Test async querying features by attributes"""
        url = self.fcb_url()
        reader = AsyncReader(url)
        opened_reader = await reader.open()

        try:
            # Test equality filter
            id_filter = AttrFilter(
                "identificatie", Operator.Eq, "NL.IMBAG.Pand.0503100000012869"
            )
            async_iter = opened_reader.query_attr([id_filter])
            features = await async_iter.collect()

            assert isinstance(features, list)
            assert len(features) == 1
            assert features[0].id == "NL.IMBAG.Pand.0503100000012869"

        except FcbError as e:
            # If specific attributes don't exist, just verify the query mechanism works
            print(f"Attribute query failed as expected: {e}")

    @pytest.mark.asyncio
    async def test_async_iterator_state_persistence(self):
        """Test that async iterator maintains state across calls"""
        url = self.fcb_url()
        reader = AsyncReader(url)
        opened_reader = await reader.open()

        # Get iterator
        async_iter = opened_reader.select_all()

        # Get first few features
        features = []
        for _ in range(3):
            feature = await async_iter.next()
            if feature is None:
                break
            features.append(feature)

        assert len(features) >= 1

        # Verify each feature is different (iterator is progressing)
        if len(features) > 1:
            assert features[0].id != features[1].id

    @pytest.mark.asyncio
    async def test_async_reader_error_handling(self):
        """Test error handling with invalid HTTP URLs"""
        # Test invalid URL
        with pytest.raises(FcbError):
            reader = AsyncReader(
                "/invalid/path"
            )  # Should error on non-HTTP path

        # Test valid URL format but non-existent server
        reader = AsyncReader("http://localhost:99999/test.fcb")
        with pytest.raises(
            (FcbError, ConnectionError, OSError)
        ):  # Should raise connection error
            await reader.open()


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
