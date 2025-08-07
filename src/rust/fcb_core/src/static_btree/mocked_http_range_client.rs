use bytes::Bytes;
use http_range_client::{self, AsyncBufferedHttpRangeClient, AsyncHttpRangeClient};
use std::cmp::min;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::ops::Range;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

/// NOTE: For debugging expediency, this test class often prefers panics over returning a result.
pub(crate) struct MockHttpRangeClient {
    path: PathBuf,
    stats: Arc<RwLock<RequestStats>>,
    buffer: Option<Bytes>,
}

pub(crate) struct RequestStats {
    pub request_count: u64,
    pub bytes_requested: u64,
}

impl RequestStats {
    pub(crate) fn new() -> Self {
        Self {
            request_count: 0,
            bytes_requested: 0,
        }
    }
}

#[async_trait::async_trait]
impl AsyncHttpRangeClient for MockHttpRangeClient {
    async fn get_range(&self, url: &str, range: &str) -> http_range_client::Result<Bytes> {
        assert_eq!(url, self.path.to_str().unwrap());
        /// This is a hack, but we need the start and length of the range
        /// since all we're given is the pre-formatted range string, we
        /// need to parse it into its components
        ///
        /// For expediency, this test code panics rather than returns a result.
        fn parse_range_header(range: &str) -> Range<u64> {
            let bytes = range.strip_prefix("bytes=").unwrap();
            let parts: Vec<&str> = bytes.split('-').collect();
            assert!(parts.len() == 2);
            let start = parts[0].parse().expect("should have valid start range");
            let end: u64 = parts[1].parse().expect("should have valid end range");
            // Range headers are *inclusive*
            start..(end + 1)
        }

        let range = parse_range_header(range);
        let request_length = range.end - range.start;
        let mut stats = self
            .stats
            .write()
            .expect("test code does not handle actual concurrency");

        stats.request_count += 1;
        stats.bytes_requested += request_length;

        if let Some(buffer) = &self.buffer {
            let start = range.start as usize;
            let end = range.end as usize;

            // Ensure we don't go out of bounds
            // assert!(end <= buffer.len(), "requested range exceeds buffer size");
            let end = min(end, buffer.len());

            Ok(buffer.slice(start..end))
        } else {
            let mut file_reader = BufReader::new(File::open(&self.path).unwrap());
            file_reader
                .seek(SeekFrom::Start(range.start))
                .expect("unable to seek test reader");
            let mut output = vec![0; request_length as usize];
            file_reader
                .read_exact(&mut output)
                .expect("failed to read from test reader");
            Ok(Bytes::from(output))
        }
    }

    async fn head_response_header(
        &self,
        _url: &str,
        _header: &str,
    ) -> http_range_client::Result<Option<String>> {
        unimplemented!()
    }
}

impl MockHttpRangeClient {
    pub fn new_mock_http_range_client(
        data: &[u8],
    ) -> AsyncBufferedHttpRangeClient<MockHttpRangeClient> {
        let stats = Arc::new(RwLock::new(RequestStats::new()));
        let client =
            MockHttpRangeClient::new_with_bytes("in-memory", Bytes::from(data.to_vec()), stats);

        let mut client = AsyncBufferedHttpRangeClient::with(client, "in-memory");
        client.set_min_req_size(0);
        client
    }

    pub fn request_count(&self) -> u64 {
        self.stats.read().unwrap().request_count
    }

    pub fn bytes_requested(&self) -> u64 {
        self.stats.read().unwrap().bytes_requested
    }

    fn new(path: &str) -> Self {
        let stats = Arc::new(RwLock::new(RequestStats::new()));
        Self {
            path: path.into(),
            stats,
            buffer: None,
        }
    }

    /// Creates a new MockHttpRangeClient with in-memory data
    pub fn new_with_bytes(path: &str, data: Bytes, stats: Arc<RwLock<RequestStats>>) -> Self {
        Self {
            path: path.into(),
            stats,
            buffer: Some(data),
        }
    }

    /// Reads file contents into memory buffer
    pub fn load_file_into_memory(&mut self) -> std::io::Result<()> {
        let mut file = File::open(&self.path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        self.buffer = Some(Bytes::from(buffer));
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use bytes::Bytes;

    fn create_test_data(size: usize) -> Bytes {
        // Create a buffer of consecutive u8 values (0, 1, 2, ..., 255, 0, 1, ...)
        let mut buffer = Vec::with_capacity(size);
        for i in 0..size {
            buffer.push((i % 256) as u8);
        }
        Bytes::from(buffer)
    }

    #[tokio::test]
    async fn test_mock_client_range_fetching() {
        // Create test data with 1000 bytes of consecutive values
        let test_data = create_test_data(1000);

        let mut client = MockHttpRangeClient::new_mock_http_range_client(&test_data);

        // Test cases with different ranges
        let test_cases = vec![
            (0, 10),     // Start of buffer
            (500, 520),  // Middle of buffer
            (990, 1000), // End of buffer
            (100, 400),  // Larger range
            (0, 1),      // Minimal range
        ];

        for (start, end) in test_cases {
            // Calculate the expected data for this range
            let expected = test_data.slice(start..end);

            let length = end - start;

            // Fetch the range
            let result = client.get_range(start, length).await.unwrap();

            // Verify the result matches expected data
            assert_eq!(
                result.len(),
                expected.len(),
                "Range {}-{}: returned length {} doesn't match expected length {}",
                start,
                end,
                result.len(),
                expected.len()
            );

            assert_eq!(
                result, expected,
                "Range {start}-{end}: returned data doesn't match expected data"
            );

            // Verify each byte individually for clarity in case of failure
            for i in 0..result.len() {
                assert_eq!(
                    result[i], expected[i],
                    "Range {}-{}: Mismatch at position {}: got {} expected {}",
                    start, end, i, result[i], expected[i]
                );
            }
        }
    }
}
