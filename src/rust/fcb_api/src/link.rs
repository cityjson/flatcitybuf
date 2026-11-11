use crate::{handlers::BboxQuery, models::Link};

/// Advertise building resources only. Building parts are embedded CityObjects,
/// so their hierarchy belongs in the feature rather than HTTP navigation links.
pub fn build_feature_links(
    base_url: &str,
    collection_id: &str,
    requested_id: &str,
    crs: Option<&str>,
) -> Vec<Link> {
    use crate::constants::*;
    let mut collection_url = reqwest::Url::parse(base_url).expect("validated base URL");
    collection_url
        .path_segments_mut()
        .expect("HTTP base URL")
        .pop_if_empty()
        .extend(["collections", collection_id]);
    let mut self_url = collection_url.clone();
    self_url
        .path_segments_mut()
        .expect("HTTP base URL")
        .extend(["items", requested_id]);
    if let Some(crs) = crs {
        self_url.query_pairs_mut().append_pair("crs", crs);
    }
    vec![
        Link {
            href: self_url.into(),
            rel: REL_SELF.into(),
            r#type: Some(CONTENT_TYPE_JSON.into()),
            title: Some("Feature details".into()),
            ..Default::default()
        },
        Link {
            href: collection_url.to_string(),
            rel: REL_COLLECTION.into(),
            r#type: Some(CONTENT_TYPE_JSON.into()),
            ..Default::default()
        },
    ]
}

pub fn build_link_json(
    base_url: &str,
    collection_id: &str,
    query: &BboxQuery,
    limit: usize,
    offset: usize,
    number_matched: usize,
    number_returned: usize,
) -> Vec<Link> {
    let mut pages = vec![("self", offset), ("first", 0)];
    if offset > 0 {
        pages.push(("prev", offset.saturating_sub(limit)));
    }
    if offset.saturating_add(number_returned) < number_matched {
        if let Some(next) = offset.checked_add(limit) {
            pages.push(("next", next));
        }
    }
    if number_matched > 0 {
        let last = (number_matched - 1) / limit * limit;
        if last > 0 {
            pages.push(("last", last));
        }
    }
    pages
        .into_iter()
        .map(|(rel, offset)| {
            let mut url = reqwest::Url::parse(&format!(
                "{}/collections/{collection_id}/items",
                base_url.trim().trim_end_matches('/')
            ))
            .expect("validated base URL");
            {
                let mut pairs = url.query_pairs_mut();
                pairs
                    .append_pair("limit", &limit.to_string())
                    .append_pair("offset", &offset.to_string());
                for (key, value) in [
                    ("bbox", &query.bbox),
                    ("bbox-crs", &query.bbox_crs),
                    ("filter", &query.filter),
                    ("f", &query.f),
                    ("crs", &query.crs),
                ] {
                    if let Some(value) = value {
                        pairs.append_pair(key, value);
                    }
                }
            }
            Link {
                href: url.into(),
                rel: rel.into(),
                r#type: Some("application/json".into()),
                ..Default::default()
            }
        })
        .collect()
}
pub fn build_link_header(
    base_url: &str,
    collection_id: &str,
    query: &BboxQuery,
    limit: usize,
    offset: usize,
    number_matched: usize,
    number_returned: usize,
) -> String {
    build_link_json(
        base_url,
        collection_id,
        query,
        limit,
        offset,
        number_matched,
        number_returned,
    )
    .iter()
    .map(|l| format!("<{}>; rel=\"{}\"", l.href, l.rel))
    .collect::<Vec<_>>()
    .join(", ")
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_links_encode_item_ids_and_preserve_crs() {
        let id = "building /?#% café";
        let links = build_feature_links("https://api.test/v1/", "pand", id, Some("EPSG:7415"));
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].rel, "self");
        assert_eq!(links[1].rel, "collection");
        assert_eq!(links[1].href, "https://api.test/v1/collections/pand");
        let self_url = reqwest::Url::parse(&links[0].href).unwrap();
        assert_eq!(self_url.fragment(), None);
        assert_eq!(
            self_url.query_pairs().collect::<Vec<_>>(),
            vec![("crs".into(), "EPSG:7415".into())]
        );
        let segments: Vec<_> = self_url.path_segments().unwrap().collect();
        assert_eq!(segments.len(), 5);
        assert_eq!(urlencoding::decode(segments[4]).unwrap(), id);
        let links = build_feature_links("https://api.test", "pand", "building", None);
        assert_eq!(
            links[0].href,
            "https://api.test/collections/pand/items/building"
        );
    }

    #[test]
    fn encoded_links_and_boundaries() {
        let filter = "name = 'a & b+% café'";
        let q: BboxQuery = serde_json::from_value(serde_json::json!({"filter":filter})).unwrap();
        for l in build_link_json("http://example.test/", "pand", &q, 10, 0, 25, 10) {
            let u = reqwest::Url::parse(&l.href).unwrap();
            assert_eq!(
                u.query_pairs().find(|(k, _)| k == "filter").unwrap().1,
                filter
            );
        }
        let empty = build_link_json("http://example.test", "pand", &q, 10, 0, 0, 0);
        assert_eq!(empty.len(), 2);
        let huge = build_link_json("http://example.test", "pand", &q, 10, usize::MAX, 5, 0);
        assert!(!huge.iter().any(|l| l.rel == "next"));
    }
}
