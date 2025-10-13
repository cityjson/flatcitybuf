use crate::handlers::BboxQuery;
use crate::models::Link;

/// Build Link header value for pagination
/// Returns a string formatted according to RFC 8288 (Web Linking)
pub fn build_link_header(
    base_url: &str,
    collection_id: &str,
    query: &BboxQuery,
    limit: i32,
    offset: i32,
    number_matched: i32,
    number_returned: i32,
) -> String {
    let mut query_params = vec![];
    if let Some(bbox_str) = &query.bbox {
        query_params.push(format!("bbox={bbox_str}"));
    }
    if let Some(bbox_crs) = &query.bbox_crs {
        query_params.push(format!("bbox-crs={bbox_crs}"));
    }
    if let Some(filter_str) = &query.filter {
        query_params.push(format!("filter={filter_str}"));
    }
    if let Some(f) = &query.f {
        query_params.push(format!("f={f}"));
    }

    let query_suffix = if query_params.is_empty() {
        String::new()
    } else {
        format!("&{}", query_params.join("&"))
    };

    let mut links = vec![];

    let base_url = base_url.trim().trim_end_matches('/');

    // Self link
    links.push(format!(
        "<{}/collections/{}/items?limit={}&offset={}{}>; rel=\"self\"",
        base_url, collection_id, limit, offset, query_suffix
    ));

    // First link
    links.push(format!(
        "<{}/collections/{}/items?limit={}&offset=0{}>; rel=\"first\"",
        base_url, collection_id, limit, query_suffix
    ));

    // Prev link
    if offset > 0 {
        let prev_offset = (offset - limit).max(0);
        links.push(format!(
            "<{}/collections/{}/items?limit={}&offset={}{}>; rel=\"prev\"",
            base_url, collection_id, limit, prev_offset, query_suffix
        ));
    }

    // Next link
    if offset + number_returned < number_matched {
        let next_offset = offset + limit;
        links.push(format!(
            "<{}/collections/{}/items?limit={}&offset={}{}>; rel=\"next\"",
            base_url, collection_id, limit, next_offset, query_suffix
        ));
    }

    // Last link
    let last_offset = ((number_matched - 1) / limit) * limit;
    if last_offset > 0 {
        links.push(format!(
            "<{}/collections/{}/items?limit={}&offset={}{}>; rel=\"last\"",
            base_url, collection_id, limit, last_offset, query_suffix
        ));
    }

    links.join(", ")
}

/// Build JSON link objects for pagination
/// Returns a vector of Link objects for use in JSON responses
pub fn build_link_json(
    base_url: &str,
    collection_id: &str,
    query: &BboxQuery,
    limit: i32,
    offset: i32,
    number_matched: i32,
    number_returned: i32,
) -> Vec<Link> {
    let mut query_params = vec![];
    if let Some(bbox_str) = &query.bbox {
        query_params.push(format!("bbox={bbox_str}"));
    }
    if let Some(bbox_crs) = &query.bbox_crs {
        query_params.push(format!("bbox-crs={bbox_crs}"));
    }
    if let Some(filter_str) = &query.filter {
        query_params.push(format!("filter={filter_str}"));
    }
    if let Some(f) = &query.f {
        query_params.push(format!("f={f}"));
    }

    let query_suffix = if query_params.is_empty() {
        String::new()
    } else {
        format!("&{}", query_params.join("&"))
    };

    let mut links = vec![];

    let base_url = base_url.trim().trim_end_matches('/');

    // Self link
    links.push(Link {
        href: format!(
            "{}/collections/{}/items?limit={}&offset={}{}",
            base_url, collection_id, limit, offset, query_suffix
        ),
        rel: "self".to_string(),
        r#type: Some("application/json".to_string()),
        title: Some("this document".to_string()),
        ..Default::default()
    });

    // First link
    links.push(Link {
        href: format!(
            "{}/collections/{}/items?limit={}&offset=0{}",
            base_url, collection_id, limit, query_suffix
        ),
        rel: "first".to_string(),
        r#type: Some("application/json".to_string()),
        title: Some("First page".to_string()),
        ..Default::default()
    });

    // Prev link
    if offset > 0 {
        let prev_offset = (offset - limit).max(0);
        links.push(Link {
            href: format!(
                "{}/collections/{}/items?limit={}&offset={}{}",
                base_url, collection_id, limit, prev_offset, query_suffix
            ),
            rel: "prev".to_string(),
            r#type: Some("application/json".to_string()),
            title: Some("Previous page".to_string()),
            ..Default::default()
        });
    }

    // Next link
    if offset + number_returned < number_matched {
        let next_offset = offset + limit;
        links.push(Link {
            href: format!(
                "{}/collections/{}/items?limit={}&offset={}{}",
                base_url, collection_id, limit, next_offset, query_suffix
            ),
            rel: "next".to_string(),
            r#type: Some("application/json".to_string()),
            title: Some("Next page".to_string()),
            ..Default::default()
        });
    }

    // Last link
    let last_offset = ((number_matched - 1) / limit) * limit;
    if last_offset > 0 {
        links.push(Link {
            href: format!(
                "{}/collections/{}/items?limit={}&offset={}{}",
                base_url, collection_id, limit, last_offset, query_suffix
            ),
            rel: "last".to_string(),
            r#type: Some("application/json".to_string()),
            title: Some("Last page".to_string()),
            ..Default::default()
        });
    }

    links
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_query(bbox: Option<&str>, filter: Option<&str>, f: Option<&str>) -> BboxQuery {
        BboxQuery {
            limit: None,
            offset: None,
            bbox: bbox.map(|s| s.to_string()),
            bbox_crs: None,
            filter: filter.map(|s| s.to_string()),
            f: f.map(|s| s.to_string()),
        }
    }

    #[test]
    fn test_build_link_header_first_page() {
        let query = create_test_query(None, None, None);
        let result =
            build_link_header("http://localhost:8080", "buildings", &query, 10, 0, 100, 10);

        assert!(result.contains("rel=\"self\""));
        assert!(result.contains("rel=\"first\""));
        assert!(result.contains("rel=\"next\""));
        assert!(result.contains("rel=\"last\""));
        assert!(!result.contains("rel=\"prev\"")); // No prev on first page
    }

    #[test]
    fn test_build_link_header_middle_page() {
        let query = create_test_query(None, None, None);
        let result = build_link_header(
            "http://localhost:8080",
            "buildings",
            &query,
            10,
            20,
            100,
            10,
        );

        assert!(result.contains("rel=\"self\""));
        assert!(result.contains("rel=\"first\""));
        assert!(result.contains("rel=\"prev\""));
        assert!(result.contains("rel=\"next\""));
        assert!(result.contains("rel=\"last\""));
    }

    #[test]
    fn test_build_link_header_last_page() {
        let query = create_test_query(None, None, None);
        let result = build_link_header(
            "http://localhost:8080",
            "buildings",
            &query,
            10,
            90,
            100,
            10,
        );

        assert!(result.contains("rel=\"self\""));
        assert!(result.contains("rel=\"first\""));
        assert!(result.contains("rel=\"prev\""));
        assert!(!result.contains("rel=\"next\"")); // No next on last page
        assert!(result.contains("rel=\"last\""));
    }

    #[test]
    fn test_build_link_header_single_page() {
        let query = create_test_query(None, None, None);
        let result = build_link_header("http://localhost:8080", "buildings", &query, 10, 0, 5, 5);

        assert!(result.contains("rel=\"self\""));
        assert!(result.contains("rel=\"first\""));
        assert!(!result.contains("rel=\"prev\"")); // No prev on first page
        assert!(!result.contains("rel=\"next\"")); // No next when all results fit
        assert!(!result.contains("rel=\"last\"")); // Last offset is 0, so not included
    }

    #[test]
    fn test_build_link_header_with_query_params() {
        let query = create_test_query(
            Some("1.0,2.0,3.0,4.0"),
            Some("building_type='residential'"),
            Some("json"),
        );
        let result =
            build_link_header("http://localhost:8080", "buildings", &query, 10, 0, 100, 10);

        assert!(result.contains("bbox=1.0,2.0,3.0,4.0"));
        assert!(result.contains("filter=building_type='residential'"));
        assert!(result.contains("f=json"));
    }

    #[test]
    fn test_build_link_header_base_url_trimming() {
        let query = create_test_query(None, None, None);
        let result = build_link_header(
            "http://localhost:8080/",
            "buildings",
            &query,
            10,
            0,
            100,
            10,
        );

        // Should not have double slashes
        assert!(!result.contains("8080//collections"));
        assert!(result.contains("http://localhost:8080/collections"));
    }

    #[test]
    fn test_build_link_header_offsets() {
        let query = create_test_query(None, None, None);
        let result = build_link_header(
            "http://localhost:8080",
            "buildings",
            &query,
            10,
            25,
            100,
            10,
        );

        // Check that offset values are correct
        assert!(result.contains("offset=25")); // self
        assert!(result.contains("offset=0")); // first
        assert!(result.contains("offset=15")); // prev (25 - 10)
        assert!(result.contains("offset=35")); // next (25 + 10)
        assert!(result.contains("offset=90")); // last ((100-1)/10)*10
    }

    #[test]
    fn test_build_link_json_first_page() {
        let query = create_test_query(None, None, None);
        let links = build_link_json("http://localhost:8080", "buildings", &query, 10, 0, 100, 10);

        assert_eq!(links.len(), 4); // self, first, next, last
        assert_eq!(links[0].rel, "self");
        assert_eq!(links[1].rel, "first");
        assert_eq!(links[2].rel, "next");
        assert_eq!(links[3].rel, "last");
    }

    #[test]
    fn test_build_link_json_middle_page() {
        let query = create_test_query(None, None, None);
        let links = build_link_json(
            "http://localhost:8080",
            "buildings",
            &query,
            10,
            20,
            100,
            10,
        );

        assert_eq!(links.len(), 5); // self, first, prev, next, last
        assert_eq!(links[0].rel, "self");
        assert_eq!(links[1].rel, "first");
        assert_eq!(links[2].rel, "prev");
        assert_eq!(links[3].rel, "next");
        assert_eq!(links[4].rel, "last");
    }

    #[test]
    fn test_build_link_json_last_page() {
        let query = create_test_query(None, None, None);
        let links = build_link_json(
            "http://localhost:8080",
            "buildings",
            &query,
            10,
            90,
            100,
            10,
        );

        assert_eq!(links.len(), 4); // self, first, prev, last
        assert_eq!(links[0].rel, "self");
        assert_eq!(links[1].rel, "first");
        assert_eq!(links[2].rel, "prev");
        assert_eq!(links[3].rel, "last");
    }

    #[test]
    fn test_build_link_json_single_page() {
        let query = create_test_query(None, None, None);
        let links = build_link_json("http://localhost:8080", "buildings", &query, 10, 0, 5, 5);

        assert_eq!(links.len(), 2); // self, first only
        assert_eq!(links[0].rel, "self");
        assert_eq!(links[1].rel, "first");
    }

    #[test]
    fn test_build_link_json_with_query_params() {
        let query = create_test_query(
            Some("1.0,2.0,3.0,4.0"),
            Some("building_type='residential'"),
            Some("json"),
        );
        let links = build_link_json("http://localhost:8080", "buildings", &query, 10, 0, 100, 10);

        // Check that all links contain query parameters
        for link in &links {
            assert!(link.href.contains("bbox=1.0,2.0,3.0,4.0"));
            assert!(link.href.contains("filter=building_type='residential'"));
            assert!(link.href.contains("f=json"));
        }
    }

    #[test]
    fn test_build_link_json_url_structure() {
        let query = create_test_query(None, None, None);
        let links = build_link_json("http://localhost:8080", "buildings", &query, 10, 0, 100, 10);

        let self_link = &links[0];
        assert_eq!(self_link.rel, "self");
        assert_eq!(
            self_link.href,
            "http://localhost:8080/collections/buildings/items?limit=10&offset=0"
        );
        assert_eq!(self_link.r#type, Some("application/json".to_string()));
        assert_eq!(self_link.title, Some("this document".to_string()));
    }

    #[test]
    fn test_build_link_json_prev_offset_calculation() {
        let query = create_test_query(None, None, None);

        // Test when offset < limit (prev should be 0)
        let links = build_link_json("http://localhost:8080", "buildings", &query, 10, 5, 100, 10);
        let prev_link = links.iter().find(|l| l.rel == "prev").unwrap();
        assert!(prev_link.href.contains("offset=0"));

        // Test normal case
        let links = build_link_json(
            "http://localhost:8080",
            "buildings",
            &query,
            10,
            25,
            100,
            10,
        );
        let prev_link = links.iter().find(|l| l.rel == "prev").unwrap();
        assert!(prev_link.href.contains("offset=15"));
    }

    #[test]
    fn test_build_link_json_last_offset_calculation() {
        let query = create_test_query(None, None, None);

        // 100 items, limit 10: last offset should be 90
        let links = build_link_json("http://localhost:8080", "buildings", &query, 10, 0, 100, 10);
        let last_link = links.iter().find(|l| l.rel == "last").unwrap();
        assert!(last_link.href.contains("offset=90"));

        // 95 items, limit 10: last offset should be 90
        let links = build_link_json("http://localhost:8080", "buildings", &query, 10, 0, 95, 10);
        let last_link = links.iter().find(|l| l.rel == "last").unwrap();
        assert!(last_link.href.contains("offset=90"));

        // 91 items, limit 10: last offset should be 90
        let links = build_link_json("http://localhost:8080", "buildings", &query, 10, 0, 91, 10);
        let last_link = links.iter().find(|l| l.rel == "last").unwrap();
        assert!(last_link.href.contains("offset=90"));
    }

    #[test]
    fn test_build_link_json_base_url_trimming() {
        let query = create_test_query(None, None, None);
        let links = build_link_json(
            "http://localhost:8080/",
            "buildings",
            &query,
            10,
            0,
            100,
            10,
        );

        // Should not have double slashes
        for link in &links {
            assert!(!link.href.contains("8080//collections"));
            assert!(link.href.contains("http://localhost:8080/collections"));
        }
    }

    #[test]
    fn test_build_link_json_link_types() {
        let query = create_test_query(None, None, None);
        let links = build_link_json(
            "http://localhost:8080",
            "buildings",
            &query,
            10,
            20,
            100,
            10,
        );

        // All links should have application/json type
        for link in &links {
            assert_eq!(link.r#type, Some("application/json".to_string()));
        }

        // Check titles
        let self_link = links.iter().find(|l| l.rel == "self").unwrap();
        assert_eq!(self_link.title, Some("this document".to_string()));

        let first_link = links.iter().find(|l| l.rel == "first").unwrap();
        assert_eq!(first_link.title, Some("First page".to_string()));

        let prev_link = links.iter().find(|l| l.rel == "prev").unwrap();
        assert_eq!(prev_link.title, Some("Previous page".to_string()));

        let next_link = links.iter().find(|l| l.rel == "next").unwrap();
        assert_eq!(next_link.title, Some("Next page".to_string()));

        let last_link = links.iter().find(|l| l.rel == "last").unwrap();
        assert_eq!(last_link.title, Some("Last page".to_string()));
    }

    #[test]
    fn test_build_link_header_whitespace_trimming() {
        let query = create_test_query(None, None, None);

        // Test with leading/trailing whitespace
        let result = build_link_header(
            "  http://localhost:8080  ",
            "buildings",
            &query,
            10,
            0,
            100,
            10,
        );

        // Should not have any whitespace in URLs
        assert!(result.contains("http://localhost:8080/collections"));
        assert!(!result.contains("  http://"));
        assert!(!result.contains("8080  /"));
    }

    #[test]
    fn test_build_link_json_whitespace_trimming() {
        let query = create_test_query(None, None, None);

        // Test with leading/trailing whitespace and trailing slash
        let links = build_link_json(
            "  http://localhost:8080/  ",
            "buildings",
            &query,
            10,
            0,
            100,
            10,
        );

        // All links should have clean URLs without whitespace
        for link in &links {
            assert!(link.href.starts_with("http://localhost:8080/"));
            assert!(!link.href.contains("  "));
            assert!(!link.href.contains("8080//"));
        }
    }
}
