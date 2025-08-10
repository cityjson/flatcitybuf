"""Tests for FlatCityBuf Python bindings"""

import pytest
import fcb
from fcb import Reader, BBox, AttrFilter, FcbError


class TestReader:
    """Test Reader functionality"""
    
    def test_reader_creation_nonexistent_file(self):
        """Test that Reader raises appropriate error for non-existent file"""
        with pytest.raises(FcbError):
            Reader("/path/that/does/not/exist.fcb")
    
    def test_reader_creation_http_url(self):
        """Test that Reader raises error for HTTP URLs (should use AsyncReader)"""
        with pytest.raises(FcbError) as exc_info:
            Reader("https://example.com/file.fcb")
        assert "HTTP URLs not supported in sync Reader" in str(exc_info.value)
    
    def test_bbox_creation_and_methods(self):
        """Test BBox creation and methods"""
        bbox = BBox(0.0, 0.0, 100.0, 100.0)
        
        assert bbox.min_x == 0.0
        assert bbox.min_y == 0.0
        assert bbox.max_x == 100.0
        assert bbox.max_y == 100.0
        
        # Test contains
        assert bbox.contains(50.0, 50.0)
        assert not bbox.contains(150.0, 50.0)
        
        # Test intersects
        other_bbox = BBox(50.0, 50.0, 150.0, 150.0)
        assert bbox.intersects(other_bbox)
        
        non_intersecting = BBox(200.0, 200.0, 300.0, 300.0)
        assert not bbox.intersects(non_intersecting)
        
        # Test area
        assert bbox.area() == 10000.0
    
    def test_attr_filter_creation(self):
        """Test AttrFilter creation and class methods"""
        # Test constructor
        filter1 = AttrFilter("height", fcb.Operator.Gt, 50.0)
        assert filter1.field == "height"
        
        # Test class methods
        eq_filter = AttrFilter.eq("type", "building")
        assert eq_filter.field == "type"
        
        gt_filter = AttrFilter.gt("height", 100.0)
        assert gt_filter.field == "height"
    
    def test_convenience_functions(self):
        """Test module-level convenience functions"""
        # These will fail with non-existent files, but we can test they exist
        assert hasattr(fcb, 'open_file')
        assert hasattr(fcb, 'query_bbox')
        
        with pytest.raises(FcbError):
            fcb.open_file("/nonexistent.fcb")
        
        with pytest.raises(FcbError):
            fcb.query_bbox("/nonexistent.fcb", 0, 0, 100, 100)

    def test_imports(self):
        """Test that all expected classes can be imported"""
        from fcb import (
            Reader, AsyncReader, Feature, Geometry, Vertex, 
            FileInfo, BBox, AttrFilter, Operator, FcbError
        )
        
        # All imports should succeed
        assert Reader is not None
        assert AsyncReader is not None
        assert Feature is not None
        assert Geometry is not None
        assert Vertex is not None
        assert FileInfo is not None
        assert BBox is not None
        assert AttrFilter is not None
        assert Operator is not None
        assert FcbError is not None


# TODO: Add integration tests with actual FCB files
# These would require test data files to be available

class TestIntegration:
    """Integration tests with actual FCB files"""
    
    @property
    def test_data_dir(self):
        """Get path to test data directory"""
        import os
        current_dir = os.path.dirname(os.path.abspath(__file__))
        return os.path.join(current_dir, "..", "..", "fcb_core", "tests", "data")
    
    def test_read_actual_file(self):
        """Test reading an actual FCB file"""
        import os
        small_fcb_path = os.path.join(self.test_data_dir, "small.fcb")
        
        if not os.path.exists(small_fcb_path):
            pytest.skip("Test FCB file not available")
        
        reader = Reader(small_fcb_path)
        info = reader.info()
        assert info.feature_count > 0
        
        # Test iteration
        features = list(reader)
        assert len(features) > 0
        
        # Test spatial query (use a large bbox to catch some features)
        bbox_features = reader.query_bbox(-1000, -1000, 100000, 100000)
        assert isinstance(bbox_features, list)
    
    @pytest.mark.skip(reason="Requires HTTP test setup")  
    def test_async_reader(self):
        """Test AsyncReader with HTTP URL"""
        # This would require a test HTTP endpoint
        async_reader = fcb.AsyncReader("https://example.com/test.fcb")
        info = async_reader.info()
        assert info is not None