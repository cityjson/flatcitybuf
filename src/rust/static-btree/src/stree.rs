use crate::entry::Offset;
use crate::error::Error;
use crate::{Entry, Key};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use core::f64;
// #[cfg(feature = "http")]
// use http_range_client::{AsyncBufferedHttpRangeClient, AsyncHttpRangeClient};
use std::cmp::{min, Ordering};
use std::collections::{HashMap, VecDeque};
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::mem::size_of;
use std::ops::Range;

// This implementation was derived from FlatGeobuf's implemenation.

/// S-Tree node
pub type NodeItem<K: Key> = Entry<K>;

/// S-Tree node. NodeItem's offset is the offset to the actual offset section in the file. This is to support duplicate keys.
impl<K: Key> NodeItem<K> {
    pub fn new_with_key(key: K) -> NodeItem<K> {
        NodeItem { key, offset: 0 }
    }

    pub fn create(offset: u64) -> NodeItem<K> {
        NodeItem {
            key: K::default(),
            offset,
        }
    }

    fn from_bytes(raw: &[u8]) -> Result<Self, Error> {
        Self::read_from(&mut Cursor::new(raw))
    }

    pub fn write<W: Write>(&self, wtr: &mut W) -> std::io::Result<()> {
        self.write_to(wtr);
        Ok(())
    }

    pub fn set_key(&mut self, key: K) {
        self.key = key;
    }

    pub fn set_offset(&mut self, offset: u64) {
        self.offset = offset;
    }

    pub fn equals(&self, other: &NodeItem<K>) -> bool {
        self.key == other.key
    }
}

/// Read full capacity of vec from data stream
fn read_node_vec<K: Key>(
    node_items: &mut Vec<NodeItem<K>>,
    mut data: impl Read,
) -> Result<(), Error> {
    node_items.clear();
    for _ in 0..node_items.capacity() {
        node_items.push(NodeItem::read_from(&mut data)?);
    }
    Ok(())
}

/// Read partial item vec from data stream
fn read_node_items<K: Key, R: Read + Seek>(
    data: &mut R,
    base: u64,
    node_index: usize,
    length: usize,
) -> Result<Vec<NodeItem<K>>, Error> {
    let mut node_items = Vec::with_capacity(length);
    data.seek(SeekFrom::Start(
        base + (node_index * NodeItem::<K>::SERIALIZED_SIZE) as u64,
    ))?;
    read_node_vec(&mut node_items, data)?;
    Ok(node_items)
}

// /// Read partial item vec from http
// #[cfg(feature = "http")]
// async fn read_http_node_items<T: AsyncHttpRangeClient>(
//     client: &mut AsyncBufferedHttpRangeClient<T>,
//     base: usize,
//     node_ids: &Range<usize>,
// ) -> Result<Vec<NodeItem>, Error> {
//     let begin = base + node_ids.start * size_of::<NodeItem>();
//     let length = node_ids.len() * size_of::<NodeItem>();
//     let bytes = client
//         // we've  already determined precisely which nodes to fetch - no need for extra.
//         .min_req_size(0)
//         .get_range(begin, length)
//         .await?;

//     let mut node_items = Vec::with_capacity(node_ids.len());
//     debug_assert_eq!(bytes.len(), length);
//     for node_item_bytes in bytes.chunks(size_of::<NodeItem>()) {
//         node_items.push(NodeItem::from_bytes(node_item_bytes)?);
//     }
//     Ok(node_items)
// }

#[derive(Debug)]
/// Bbox filter search result
pub struct SearchResultItem {
    /// Byte offset in feature data section
    pub offset: usize,
    /// Feature number
    pub index: usize,
}

/// S-Tree
pub struct Stree<K: Key> {
    node_items: Vec<NodeItem<K>>,
    num_original_items: usize, // number of entries given, this will allow duplicates
    num_leaf_nodes: usize, // number of leaf nodes actually stored, this doesn't allow duplicates
    branching_factor: u16,
    level_bounds: Vec<Range<usize>>,
    payload_start: usize, // offset of the payload in the file. The payload is the buffer where actual data offsets are stored.
}

impl<K: Key> Stree<K> {
    pub const DEFAULT_NODE_SIZE: u16 = 16;

    // branching_factor is the number of children per node, it'll be B and node_size is B-1
    fn init(&mut self, branching_factor: u16) -> Result<(), Error> {
        assert!(branching_factor >= 2, "Branching factor must be at least 2");
        assert!(self.num_leaf_nodes > 0, "Cannot create empty tree");
        self.branching_factor = branching_factor.clamp(2u16, 65535u16);
        println!("branching_factor: {branching_factor}");
        self.level_bounds =
            Stree::<K>::generate_level_bounds(self.num_leaf_nodes, self.branching_factor);
        let num_nodes = self
            .level_bounds
            .first()
            .expect("Btree has at least one level when node_size >= 2 and num_items > 0")
            .end;
        self.node_items = vec![NodeItem::create(0); num_nodes]; // Quite slow!
        Ok(())
    }

    // node_size is the number of items in each node, it'll be B-1
    fn generate_level_bounds(num_items: usize, branching_factor: u16) -> Vec<Range<usize>> {
        assert!(branching_factor >= 2, "Node size must be at least 2");
        assert!(num_items > 0, "Cannot create empty tree");
        assert!(
            num_items <= usize::MAX - ((num_items / branching_factor as usize) * 2),
            "Number of items too large"
        );

        // number of nodes per level in bottom-up order
        let mut level_num_nodes: Vec<usize> = Vec::new();
        let mut n = num_items;
        let mut num_nodes = n;
        level_num_nodes.push(n);
        loop {
            n = n.div_ceil(branching_factor as usize);
            num_nodes += n;
            level_num_nodes.push(n);
            if n < branching_factor as usize {
                break;
            }
        }

        println!("level_num_nodes: {level_num_nodes:?}");
        // bounds per level in reversed storage order (top-down)
        let mut level_offsets: Vec<usize> = Vec::with_capacity(level_num_nodes.len());
        n = num_nodes;
        for size in &level_num_nodes {
            level_offsets.push(n - size);
            n -= size;
        }
        println!("level_offsets: {level_offsets:?}");
        let mut level_bounds = Vec::with_capacity(level_num_nodes.len());
        for i in 0..level_num_nodes.len() {
            level_bounds.push(level_offsets[i]..level_offsets[i] + level_num_nodes[i]);
        }
        println!("level_bounds: {level_bounds:?}");
        level_bounds
    }

    fn generate_nodes(&mut self) -> Result<(), Error> {
        let node_size = self.branching_factor as usize - 1;
        let mut parent_min_key = HashMap::<usize, K>::new(); // key is the parent node's index, value is the minimum key of the right children node's leaf node
        for level in 0..self.level_bounds.len() - 1 {
            let children_level = &self.level_bounds[level];
            let parent_level = &self.level_bounds[level + 1];

            let mut parent_idx = parent_level.start;

            let mut child_idx = children_level.start;

            // Parent node's key is the minimum key of the right children node's leaf node
            // So, we need to find the minimum key of the right children node's leaf node
            // and set it as the parent node's key
            // We keep the minimum key of the tree with its index in the parent_min_key map

            while child_idx < children_level.end {
                if parent_idx >= parent_level.end {
                    break;
                }
                let child_idx_diff = child_idx - children_level.start;

                // e.g. when child_idx_diff is 0 or 1, the key won't be used by the parent node as it comes left
                let skip_size =
                    self.branching_factor as usize * (self.branching_factor as usize - 1);

                let is_right_most_child = (node_size * node_size) <= (child_idx_diff % skip_size)
                    && (child_idx_diff % skip_size)
                        < (self.branching_factor * self.branching_factor) as usize;
                let has_next_node = child_idx + node_size < children_level.end;

                if is_right_most_child {
                    child_idx += node_size;
                    continue;
                } else if !has_next_node {
                    let parent_key = K::max_value();
                    let parent_node = NodeItem::<K>::new(parent_key.clone(), child_idx as u64);
                    self.node_items[parent_idx] = parent_node;

                    let own_min = min(
                        self.node_items[child_idx].key.clone(),
                        parent_min_key
                            .get(&child_idx)
                            .unwrap_or(&K::max_value())
                            .clone(),
                    );
                    parent_min_key.insert(parent_idx, own_min);
                    parent_idx += 1;
                    child_idx += node_size;
                    continue;
                } else {
                    let right_node_idx = child_idx + node_size;

                    let is_leaf_node = child_idx >= self.num_nodes() - self.num_leaf_nodes;
                    if is_leaf_node {
                        let parent_key = if right_node_idx < children_level.end {
                            self.node_items[right_node_idx].key.clone()
                        } else {
                            K::max_value()
                        };
                        let parent_node = NodeItem::<K>::new(parent_key.clone(), child_idx as u64);
                        self.node_items[parent_idx] = parent_node;
                        parent_min_key.insert(parent_idx, self.node_items[child_idx].key.clone());
                        parent_idx += 1;
                        child_idx += node_size;
                        continue;
                    }

                    let parent_key = if right_node_idx < children_level.end {
                        parent_min_key
                            .get(&(child_idx + node_size))
                            .expect("Parent node's key is the minimum key of the right children node's leaf node")
                            .clone()
                    } else {
                        K::max_value()
                    };
                    let parent_node = NodeItem::<K>::new(parent_key.clone(), child_idx as u64);
                    self.node_items[parent_idx] = parent_node;
                    parent_min_key.insert(
                        parent_idx,
                        parent_min_key
                            .get(&child_idx)
                            .expect("Parent node's key is the minimum key of the right children node's leaf node")
                            .clone(),
                    );
                    parent_idx += 1;
                    child_idx += node_size;

                    continue;
                }
            }
        }
        Ok(())
    }

    fn read_data(&mut self, data: impl Read) -> Result<(), Error> {
        read_node_vec(&mut self.node_items, data)?;
        Ok(())
    }

    #[cfg(feature = "http")]
    async fn read_http<T: AsyncHttpRangeClient>(
        &mut self,
        client: &mut AsyncBufferedHttpRangeClient<T>,
        index_begin: usize,
    ) -> Result<(), Error> {
        let min_req_size = self.size(); // read full index at once
        let mut pos = index_begin;
        for i in 0..self.num_nodes() {
            let bytes = client
                .min_req_size(min_req_size)
                .get_range(pos, size_of::<NodeItem>())
                .await?;
            let n = NodeItem::from_bytes(bytes)?;
            self.extent.expand(&n);
            self.node_items[i] = n;
            pos += size_of::<NodeItem>();
        }
        Ok(())
    }

    fn num_nodes(&self) -> usize {
        self.node_items.len()
    }

    pub fn build(nodes: &[NodeItem<K>], branching_factor: u16) -> Result<Stree<K>, Error> {
        let branching_factor = branching_factor.clamp(2u16, 65535u16);
        println!("branching_factor: {branching_factor}");
        let mut tree = Stree::<K> {
            node_items: Vec::new(),
            num_leaf_nodes: nodes.len(),
            branching_factor,
            level_bounds: Vec::new(),
            num_original_items: nodes.len(),
            payload_start: 0,
        };
        tree.init(branching_factor)?;
        let num_nodes = tree.num_nodes();
        for (i, node) in nodes.iter().take(tree.num_leaf_nodes).cloned().enumerate() {
            tree.node_items[num_nodes - tree.num_leaf_nodes + i] = node;
        }
        tree.generate_nodes()?;
        //print tree for each level
        for level in (0..tree.level_bounds.len()).rev() {
            println!("level {level}:");
            println!(
                "{:?}",
                tree.node_items[tree.level_bounds[level].start..tree.level_bounds[level].end]
                    .to_vec()
            );
        }
        Ok(tree)
    }

    pub fn from_buf(
        data: impl Read,
        num_items: usize,
        branching_factor: u16,
    ) -> Result<Stree<K>, Error> {
        // NOTE: Since it's B+Tree, the branching factor is the number of children per node. Node size is branching factor - 1
        let branching_factor = branching_factor.clamp(2u16, 65535u16);
        let level_bounds = Stree::<K>::generate_level_bounds(num_items, branching_factor);
        let num_nodes = level_bounds
            .first()
            .expect("Btree has at least one level when node_size >= 2 and num_items > 0")
            .end;
        let mut tree = Stree::<K> {
            node_items: Vec::with_capacity(num_nodes),
            num_original_items: num_items,
            num_leaf_nodes: num_items,
            branching_factor,
            level_bounds,
            payload_start: 0,
        };
        tree.read_data(data)?;
        Ok(tree)
    }

    // #[cfg(feature = "http")]
    // pub async fn from_http<T: AsyncHttpRangeClient>(
    //     client: &mut AsyncBufferedHttpRangeClient<T>,
    //     index_begin: usize,
    //     num_items: usize,
    //     node_size: u16,
    // ) -> Result<Stree, Error> {
    //     let mut tree = Stree {
    //         extent: NodeItem::create(0),
    //         node_items: Vec::new(),
    //         num_leaf_nodes: num_items,
    //         branching_factor: 0,
    //         level_bounds: Vec::new(),
    //     };
    //     tree.init(node_size)?;
    //     tree.read_http(client, index_begin).await?;
    //     Ok(tree)
    // }

    pub fn find_exact(&self, key: K) -> Result<Vec<SearchResultItem>, Error> {
        let leaf_nodes_offset = self
            .level_bounds
            .first()
            .expect("RTree has at least one level when node_size >= 2 and num_items > 0")
            .start;
        let search_entry = NodeItem::new_with_key(key);
        let mut results = Vec::new();
        let mut queue = VecDeque::new();
        let node_size = self.branching_factor as usize - 1;

        queue.push_back((0, self.level_bounds.len() - 1));
        while let Some(next) = queue.pop_front() {
            let node_index = next.0;
            let level = next.1;
            let is_leaf_node = node_index >= self.num_nodes() - self.num_leaf_nodes;
            // find the end index of the node
            let end = min(node_index + node_size, self.level_bounds[level].end);

            let node_items = &self.node_items[node_index..end];
            // binary search for the search_entry. If found, delve into the child node. If search key is less than the first item, delve into the leftmost child node. If search key is greater than the last item, delve into the rightmost child node.

            if !is_leaf_node {
                let search_result =
                    node_items.binary_search_by(|item| item.key.cmp(&search_entry.key));
                match search_result {
                    Ok(index) => {
                        queue.push_back((node_items[index].offset as usize + node_size, level - 1));
                    }
                    Err(index) => {
                        if index == 0 {
                            queue.push_back((node_items[0].offset as usize, level - 1));
                        } else if index == node_items.len() {
                            queue.push_back((
                                node_items[node_items.len() - 1].offset as usize + node_size,
                                level - 1,
                            ));
                        } else {
                            queue.push_back((node_items[index].offset as usize, level - 1));
                        }
                    }
                }
            }

            if is_leaf_node {
                let result = node_items.binary_search_by(|item| item.key.cmp(&search_entry.key));
                match result {
                    Ok(index) => {
                        results.push(SearchResultItem {
                            offset: node_items[index].offset as usize,
                            index: node_items[index].offset as usize, //TODO: check if this is correct
                        });
                    }
                    Err(_) => {
                        continue;
                    }
                }
            }
        }
        Ok(results)
    }

    pub fn find_range(&self, lower: K, upper: K) -> Result<Vec<SearchResultItem>, Error> {
        let mut results = Vec::new();

        Ok(results)
    }

    pub fn size(&self) -> usize {
        self.num_nodes() * Entry::<K>::SERIALIZED_SIZE
    }

    pub fn index_size(num_items: usize, node_size: u16) -> usize {
        assert!(node_size >= 2, "Node size must be at least 2");
        assert!(num_items > 0, "Cannot create empty tree");
        let node_size_min = node_size.clamp(2, 65535) as usize;
        // limit so that resulting size in bytes can be represented by uint64_t
        // assert!(
        //     num_items <= 1 << 56,
        //     "Number of items must be less than 2^56"
        // );
        let mut n = num_items;
        let mut num_nodes = n;
        loop {
            n = n.div_ceil(node_size_min);
            num_nodes += n;
            if n == 1 {
                break;
            }
        }
        num_nodes * Entry::<K>::SERIALIZED_SIZE
    }

    /// Write all index nodes
    pub fn stream_write<W: Write>(&self, out: &mut W) -> std::io::Result<()> {
        for item in &self.node_items {
            item.write(out)?;
        }
        Ok(())
    }

    // pub fn root(&self) -> NodeItem<K> {
    //     self.root.clone()
    // }
}

#[cfg(feature = "http")]
pub mod http {
    use std::ops::{Range, RangeFrom};

    /// Byte range within a file. Suitable for an HTTP Range request.
    #[derive(Debug, Clone)]
    pub enum HttpRange {
        Range(Range<usize>),
        RangeFrom(RangeFrom<usize>),
    }

    impl HttpRange {
        pub fn start(&self) -> usize {
            match self {
                Self::Range(range) => range.start,
                Self::RangeFrom(range) => range.start,
            }
        }

        pub fn end(&self) -> Option<usize> {
            match self {
                Self::Range(range) => Some(range.end),
                Self::RangeFrom(_) => None,
            }
        }

        pub fn with_end(self, end: Option<usize>) -> Self {
            match end {
                Some(end) => Self::Range(self.start()..end),
                None => Self::RangeFrom(self.start()..),
            }
        }

        pub fn length(&self) -> Option<usize> {
            match self {
                Self::Range(range) => Some(range.end - range.start),
                Self::RangeFrom(_) => None,
            }
        }
    }

    #[derive(Debug)]
    /// Bbox filter search result
    pub struct HttpSearchResultItem {
        /// Byte offset in feature data section
        pub range: HttpRange,
    }
}
#[cfg(feature = "http")]
pub(crate) use http::*;

#[cfg(test)]
mod tests {
    use super::*;
    type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
    use crate::key::FixedStringKey;
    use crate::key::Key;
    #[test]
    fn tree_2items() -> Result<()> {
        let mut nodes = Vec::new();
        nodes.push(NodeItem::new(0, 0));
        nodes.push(NodeItem::new(2, 0));
        assert!(nodes[0].equals(&NodeItem::new(0, 0)));
        assert!(nodes[1].equals(&NodeItem::new(2, 2)));
        let mut offset = 0;
        for node in &mut nodes {
            node.offset = offset;
            offset += NodeItem::<u64>::SERIALIZED_SIZE as u64;
        }
        let tree = Stree::build(&nodes, 2)?;
        let list = tree.find_exact(0)?;
        assert_eq!(list.len(), 1);
        assert!(nodes[list[0].index].key == 0);

        let list = tree.find_exact(2)?;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].index, nodes[1].offset as usize);

        let list = tree.find_exact(1)?;
        assert_eq!(list.len(), 0);

        let list = tree.find_exact(3)?;
        assert_eq!(list.len(), 0);

        Ok(())
    }

    #[test]
    fn tree_19items_roundtrip_stream_search() -> Result<()> {
        let mut nodes = vec![
            NodeItem::new(0_i64, 0_u64),
            NodeItem::new(1_i64, 1_u64),
            NodeItem::new(2_i64, 2_u64),
            NodeItem::new(3_i64, 3_u64),
            NodeItem::new(4_i64, 4_u64),
            NodeItem::new(5_i64, 5_u64),
            NodeItem::new(6_i64, 6_u64),
            NodeItem::new(7_i64, 7_u64),
            NodeItem::new(8_i64, 8_u64),
            NodeItem::new(9_i64, 9_u64),
            NodeItem::new(10_i64, 10_u64),
            NodeItem::new(11_i64, 11_u64),
            NodeItem::new(12_i64, 12_u64),
            NodeItem::new(13_i64, 13_u64),
            NodeItem::new(14_i64, 14_u64),
            NodeItem::new(15_i64, 15_u64),
            NodeItem::new(16_i64, 16_u64),
            NodeItem::new(17_i64, 17_u64),
            NodeItem::new(18_i64, 18_u64),
        ];

        let mut offset = 0;
        for node in &mut nodes {
            node.offset = offset;
            offset += NodeItem::<u64>::SERIALIZED_SIZE as u64;
        }
        let tree = Stree::build(&nodes, 4)?;
        let list = tree.find_exact(10)?;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].index, nodes[10].offset as usize);

        let list = tree.find_exact(0)?;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].index, nodes[0].offset as usize);

        let list = tree.find_exact(18)?;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].index, nodes[18].offset as usize);

        // Not exists
        let list = tree.find_exact(19)?;
        assert_eq!(list.len(), 0);

        // Negative key
        let list = tree.find_exact(-1)?;
        assert_eq!(list.len(), 0);

        Ok(())
    }
    #[test]
    fn tree_generate_nodes() -> Result<()> {
        let nodes = vec![
            NodeItem::new(0_u64, 0_u64),
            NodeItem::new(1_u64, 1_u64),
            NodeItem::new(2_u64, 2_u64),
            NodeItem::new(3_u64, 3_u64),
            NodeItem::new(4_u64, 4_u64),
            NodeItem::new(5_u64, 5_u64),
            NodeItem::new(6_u64, 6_u64),
            NodeItem::new(7_u64, 7_u64),
            NodeItem::new(8_u64, 8_u64),
            NodeItem::new(9_u64, 9_u64),
            NodeItem::new(10_u64, 10_u64),
            NodeItem::new(11_u64, 11_u64),
            NodeItem::new(12_u64, 12_u64),
            NodeItem::new(13_u64, 13_u64),
            NodeItem::new(14_u64, 14_u64),
            NodeItem::new(15_u64, 15_u64),
            NodeItem::new(16_u64, 16_u64),
            NodeItem::new(17_u64, 17_u64),
            NodeItem::new(18_u64, 18_u64),
        ];

        // test with branching factor 3
        let tree = Stree::build(&nodes, 3)?;
        let keys = tree
            .node_items
            .into_iter()
            .map(|nodes| nodes.key)
            .collect::<Vec<_>>();
        let expected = vec![
            18,
            6,
            12,
            u64::MAX,
            2,
            4,
            8,
            10,
            14,
            16,
            u64::MAX,
            0,
            1,
            2,
            3,
            4,
            5,
            6,
            7,
            8,
            9,
            10,
            11,
            12,
            13,
            14,
            15,
            16,
            17,
            18,
        ];
        assert_eq!(keys, expected);

        // test with branching factor 4
        let tree = Stree::build(&nodes, 4)?;
        let keys = tree
            .node_items
            .into_iter()
            .map(|nodes| nodes.key)
            .collect::<Vec<_>>();
        let expected = vec![
            12,
            u64::MAX, //TODO: check if this is correct
            3,
            6,
            9,
            15,
            18,
            0,
            1,
            2,
            3,
            4,
            5,
            6,
            7,
            8,
            9,
            10,
            11,
            12,
            13,
            14,
            15,
            16,
            17,
            18,
        ];
        assert_eq!(keys, expected);
        Ok(())
    }
    #[test]
    fn tree_19items_roundtrip_string() -> Result<()> {
        let mut nodes = vec![
            NodeItem::new(FixedStringKey::<10>::from_str("a"), 0_u64),
            NodeItem::new(FixedStringKey::<10>::from_str("b"), 1_u64),
            NodeItem::new(FixedStringKey::<10>::from_str("c"), 2_u64),
            NodeItem::new(FixedStringKey::<10>::from_str("d"), 3_u64),
            NodeItem::new(FixedStringKey::<10>::from_str("e"), 4_u64),
            NodeItem::new(FixedStringKey::<10>::from_str("f"), 5_u64),
            NodeItem::new(FixedStringKey::<10>::from_str("g"), 6_u64),
            NodeItem::new(FixedStringKey::<10>::from_str("h"), 7_u64),
            NodeItem::new(FixedStringKey::<10>::from_str("i"), 8_u64),
            NodeItem::new(FixedStringKey::<10>::from_str("j"), 9_u64),
            NodeItem::new(FixedStringKey::<10>::from_str("k"), 10_u64),
            NodeItem::new(FixedStringKey::<10>::from_str("l"), 11_u64),
            NodeItem::new(FixedStringKey::<10>::from_str("m"), 12_u64),
            NodeItem::new(FixedStringKey::<10>::from_str("n"), 13_u64),
            NodeItem::new(FixedStringKey::<10>::from_str("o"), 14_u64),
            NodeItem::new(FixedStringKey::<10>::from_str("p"), 15_u64),
            NodeItem::new(FixedStringKey::<10>::from_str("q"), 16_u64),
            NodeItem::new(FixedStringKey::<10>::from_str("r"), 17_u64),
            NodeItem::new(FixedStringKey::<10>::from_str("s"), 18_u64),
        ];

        let mut offset = 0;
        for node in &mut nodes {
            node.offset = offset;
            offset += NodeItem::<u64>::SERIALIZED_SIZE as u64;
        }
        let tree = Stree::build(&nodes, 3)?;
        let list = tree.find_exact(FixedStringKey::<10>::from_str("k"))?;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].index, 10);

        let list = tree.find_exact(FixedStringKey::<10>::from_str("not exists"))?;
        assert_eq!(list.len(), 0);

        Ok(())
    }
}
