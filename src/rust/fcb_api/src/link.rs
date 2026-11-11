use crate::{handlers::BboxQuery, models::Link};

/// Match Flask's feature navigation: parent names the returned CityJSONFeature,
/// and children come from its root CityObject, rather than the collection listing.
pub fn build_feature_links(
    base_url: &str,
    collection_id: &str,
    requested_id: &str,
    feature: &cjseq::CityJSONFeature,
    crs: Option<&str>,
) -> Vec<Link> {
    use crate::constants::*;
    let mut collection_url = reqwest::Url::parse(base_url).expect("validated base URL");
    collection_url
        .path_segments_mut()
        .expect("HTTP base URL")
        .pop_if_empty()
        .extend(["collections", collection_id]);
    let item_url = |id: &str| {
        let mut url = collection_url.clone();
        url.path_segments_mut()
            .expect("HTTP base URL")
            .extend(["items", id]);
        url
    };
    let mut self_url = item_url(requested_id);
    if let Some(crs) = crs {
        self_url.query_pairs_mut().append_pair("crs", crs);
    }
    let mut links = vec![
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
        Link {
            href: item_url(&feature.id).into(),
            rel: REL_PARENT.into(),
            r#type: Some(CONTENT_TYPE_CITYJSON.into()),
            ..Default::default()
        },
    ];
    if let Some(children) = feature
        .city_objects
        .get(&feature.id)
        .and_then(|root| root.children.as_ref())
    {
        links.extend(children.iter().map(|id| Link {
            href: item_url(id).into(),
            rel: REL_CHILD.into(),
            r#type: Some(CONTENT_TYPE_CITYJSON.into()),
            ..Default::default()
        }));
    }
    links
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
    fn feature_links_encode_ids_and_use_returned_feature_as_parent() {
        let child = "part /?#% café";
        let mut feature: cjseq::CityJSONFeature = serde_json::from_value(serde_json::json!({
            "type": "CityJSONFeature", "id": "root", "vertices": [],
            "CityObjects": {"root": {"type": "Building", "children": [child]}}
        }))
        .unwrap();
        let links = build_feature_links("https://api.test/v1/", "pand", "alias", &feature, None);
        assert_eq!(
            links[0].href,
            "https://api.test/v1/collections/pand/items/alias"
        );
        assert_eq!(
            links[2].href,
            "https://api.test/v1/collections/pand/items/root"
        );
        let child_url = reqwest::Url::parse(&links[3].href).unwrap();
        assert_eq!(child_url.query(), None);
        assert_eq!(child_url.fragment(), None);
        let segments: Vec<_> = child_url.path_segments().unwrap().collect();
        assert_eq!(segments.len(), 5);
        assert_eq!(urlencoding::decode(segments[4]).unwrap(), child);

        feature.city_objects.get_mut("root").unwrap().children = Some(vec![]);
        assert_eq!(
            build_feature_links("https://api.test", "pand", "root", &feature, None).len(),
            3
        );
        feature.city_objects.clear();
        assert_eq!(
            build_feature_links("https://api.test", "pand", "root", &feature, None).len(),
            3
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
