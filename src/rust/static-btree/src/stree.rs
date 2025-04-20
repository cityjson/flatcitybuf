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
        println!("fn init(): num_nodes: {num_nodes}");
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

            if n == 1 {
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
        let mut parent_min_key = HashMap::<usize, NodeItem<K>>::new(); // key is the parent node's index, value is the minimum key of the right children node's leaf node
        for level in 0..self.level_bounds.len() - 1 {
            let children_level = &self.level_bounds[level];
            let parent_level = &self.level_bounds[level + 1];

            let mut parent_idx = parent_level.start;

            // for nth_child in 0..self.branching_factor as usize {
            let mut child_idx = children_level.start;

            // ---------old code
            // while child_idx < children_level.end {
            //     let mut parent_node = NodeItem::<K>::create(child_idx as u64);
            //     // TODO: check this logic. the parent node's key is the minimum key of the right children. That is the leftmost key in the right child node
            //     println!("child_idx: {child_idx}");
            //     parent_node.set_key(
            //         self.node_items[child_idx + self.branching_factor as usize - 1]
            //             .key
            //             .clone(),
            //     );
            //     println!("parent_node: {parent_node:?}");
            //     child_idx += self.branching_factor as usize;

            //     self.node_items[parent_idx] = parent_node;
            //     parent_idx += 1;
            // }
            // ---------old code

            // Parent node's key is the minimum key of the right children node's leaf node
            // So, we need to find the minimum key of the right children node's leaf node
            // and set it as the parent node's key
            // We can do this by iterating through the right children node's leaf nodes
            // and finding the minimum key

            while child_idx < children_level.end {
                let child_idx_diff = child_idx - children_level.start;

                // e.g. when child_idx_diff is 0 or 1, the key won't be used by the parent node as it comes left
                let skip_size =
                    self.branching_factor as usize * (self.branching_factor as usize - 1);

                if child_idx_diff % skip_size == 0 || child_idx_diff % skip_size == 1 {
                    child_idx += 1;
                    continue;
                } else {
                    // only when level is 0, the parent node's key is the minimum key of the right children node's leaf node.
                    // Otherwise, the parent node's key is the minimum key of the right children node's leaf node of the previous level

                    //TODO: clean this up
                    let parent_node = if level == 0 {
                        NodeItem::<K>::new_with_key(self.node_items[child_idx].key.clone())
                    } else {
                        // TODO: return error instead of panicking
                        NodeItem::<K>::new_with_key(
                            parent_min_key
                                .get(&child_idx)
                                .expect("Parent node's key is the minimum key of the right children node's leaf node")
                                .key
                                .clone(),
                        )
                    };
                    parent_min_key.insert(
                        parent_idx,
                        NodeItem::<K>::new_with_key(
                            self.node_items[child_idx - (self.branching_factor as usize - 1)]
                                .key
                                .clone(),
                        ),
                    );
                    self.node_items[parent_idx] = parent_node.clone();
                    parent_idx += 1;
                    child_idx += self.branching_factor as usize - 1; // -1 because the number of items in the node is branching_factor - 1
                }
            }
        }
        println!("nodes: {:#?}", self.node_items);
        Ok(())
    }

    fn read_data(&mut self, data: impl Read) -> Result<(), Error> {
        read_node_vec(&mut self.node_items, data)?;
        Ok(())
    }

    // #[cfg(feature = "http")]
    // async fn read_http<T: AsyncHttpRangeClient>(
    //     &mut self,
    //     client: &mut AsyncBufferedHttpRangeClient<T>,
    //     index_begin: usize,
    // ) -> Result<(), Error> {
    //     let min_req_size = self.size(); // read full index at once
    //     let mut pos = index_begin;
    //     for i in 0..self.num_nodes() {
    //         let bytes = client
    //             .min_req_size(min_req_size)
    //             .get_range(pos, size_of::<NodeItem>())
    //             .await?;
    //         let n = NodeItem::from_bytes(bytes)?;
    //         self.extent.expand(&n);
    //         self.node_items[i] = n;
    //         pos += size_of::<NodeItem>();
    //     }
    //     Ok(())
    // }

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

        queue.push_back((0, self.level_bounds.len() - 1));
        while let Some(next) = queue.pop_front() {
            let node_index = next.0;
            let level = next.1;
            let is_leaf_node = node_index >= self.num_nodes() - self.num_leaf_nodes;
            // find the end index of the node
            let end = min(
                node_index + self.branching_factor as usize,
                self.level_bounds[level].end,
            );
            println!("node_index: {node_index}, end: {end}");
            // search through child nodes
            for pos in node_index..end {
                println!("pos: {pos}");
                let node_item = &self.node_items[pos];

                // TODO: change here. For internal nodes, same as binary search, search where search key is greater than the current node key
                // If search key is less than leftmost key, search left child. If search key is greater than rightmost key, search right child.
                // Otherwise, search the middle child.
                match search_entry.cmp(node_item) {
                    Ordering::Less => {
                        queue.push_back((node_item.offset as usize, level - 1));
                    }
                    Ordering::Greater => {
                        queue.push_back((node_item.offset as usize, level - 1));
                    }
                    Ordering::Equal => {}
                }

                // if is_leaf_node {
                //     results.push(SearchResultItem {
                //         offset: node_item.offset as usize,
                //         index: pos - leaf_nodes_offset,
                //     });
                // } else {
                //     queue.push_back((node_item.offset as usize, level - 1));
                // }
            }
        }
        Ok(results)
    }
    // pub fn find_exact(&self, key: K) -> Result<Vec<SearchResultItem>, Error> {
    //     let leaf_nodes_offset = self
    //         .level_bounds
    //         .first()
    //         .expect("Btree has at least one level when node_size >= 2 and num_items > 0")
    //         .start;
    //     let search_entry = NodeItem::new_with_key(key);
    //     let mut results = Vec::new();
    //     let mut queue = VecDeque::new();
    //     queue.push_back((0, self.level_bounds.len() - 1));

    //     // Track visited leaf nodes to avoid duplicates when checking neighbors
    //     let mut visited_leaf_nodes = std::collections::HashSet::new();

    //     while let Some(next) = queue.pop_front() {
    //         let node_index = next.0;
    //         let level = next.1;
    //         let is_leaf_node = node_index >= self.num_nodes() - self.num_leaf_nodes;

    //         // Skip if we've already visited this leaf node
    //         if is_leaf_node && !visited_leaf_nodes.insert(node_index) {
    //             continue;
    //         }

    //         // Find the end index of the node
    //         let end = min(
    //             node_index + self.branching_factor as usize,
    //             self.level_bounds[level].end,
    //         );

    //         // Track if we found a match in this node
    //         let mut found_match = false;
    //         let mut match_positions = Vec::new();

    //         if is_leaf_node {
    //             // For leaf nodes, find exact matches
    //             for pos in node_index..end {
    //                 let node_item = &self.node_items[pos];
    //                 if search_entry.equals(node_item) {
    //                     found_match = true;
    //                     match_positions.push(pos);

    //                     results.push(SearchResultItem {
    //                         offset: node_item.offset as usize,
    //                         index: pos - leaf_nodes_offset,
    //                     });
    //                 }
    //             }

    //             // If we found a match, check neighboring nodes
    //             if found_match {
    //                 // Check if leftmost match is at the start of the node
    //                 // If so, check the previous node for matches at the end
    //                 if match_positions.first() == Some(&node_index)
    //                     && node_index > self.level_bounds[0].start
    //                 {
    //                     let prev_node_index = node_index - self.branching_factor as usize;
    //                     if !visited_leaf_nodes.contains(&prev_node_index) {
    //                         queue.push_back((prev_node_index, level));
    //                     }
    //                 }

    //                 // Check if rightmost match is at the end of the node
    //                 // If so, check the next node for matches at the beginning
    //                 if match_positions.last() == Some(&(end - 1)) && end < self.level_bounds[0].end
    //                 {
    //                     let next_node_index = end;
    //                     if !visited_leaf_nodes.contains(&next_node_index) {
    //                         queue.push_back((next_node_index, level));
    //                     }
    //                 }
    //             }
    //         } else {
    //             // For internal nodes, find the appropriate child node(s) to traverse
    //             // Default to leftmost child
    //             let mut chosen_child_pos = node_index;
    //             let mut found_potential_path = false;

    //             // Find all children that could contain the search key
    //             for pos in node_index..end {
    //                 let node_item = &self.node_items[pos];

    //                 // If we find an exact match in an internal node, we need to check:
    //                 // 1. The child node pointed to by this internal node
    //                 // 2. Also potentially the child node of the next internal node
    //                 if search_entry.equals(node_item) {
    //                     found_potential_path = true;
    //                     // Add this child's subtree to the search queue
    //                     queue.push_back((node_item.offset as usize, level - 1));

    //                     // If this is not the last entry in the node, we might need to check
    //                     // the next child's subtree as well (depends on the B-tree implementation)
    //                     if pos + 1 < end {
    //                         let next_node_item = &self.node_items[pos + 1];
    //                         queue.push_back((next_node_item.offset as usize, level - 1));
    //                     }
    //                 }
    //                 // For keys less than the search key, update the potential path
    //                 else if node_item.key < search_entry.key {
    //                     chosen_child_pos = pos;
    //                     found_potential_path = true;
    //                 }
    //                 // Once we find a key > search key, we've found the boundary
    //                 else {
    //                     // If we're at the first item and it's already > search_key,
    //                     // we need to go to this child
    //                     if pos == node_index {
    //                         chosen_child_pos = pos;
    //                         found_potential_path = true;
    //                     }
    //                     break;
    //                 }
    //             }

    //             // If we didn't find an exact match but found a potential path,
    //             // follow the chosen child
    //             if !found_potential_path {
    //                 // If all keys in the node are < search_key, go to the rightmost child
    //                 chosen_child_pos = end - 1;
    //                 queue.push_back((self.node_items[chosen_child_pos].offset as usize, level - 1));
    //             } else if !found_match {
    //                 // This handles the case where we found a child to traverse but no exact match
    //                 queue.push_back((self.node_items[chosen_child_pos].offset as usize, level - 1));
    //             }
    //         }
    //     }

    //     Ok(results)
    // }

    fn find_partition(&self, key: K) -> Result<SearchResultItem, Error> {
        let leaf_nodes_offset = self
            .level_bounds
            .first()
            .expect("Btree has at least one level when node_size >= 2 and num_items > 0")
            .start;
        let search_entry = NodeItem::new_with_key(key);
        let mut queue = VecDeque::new();
        queue.push_back((0, self.level_bounds.len() - 1));

        while let Some(next) = queue.pop_front() {
            let node_index = next.0;
            let level = next.1;
            let is_leaf_node = node_index >= self.num_nodes() - self.num_leaf_nodes;

            // Find the end index of the node
            let end = min(
                node_index + self.branching_factor as usize,
                self.level_bounds[level].end,
            );

            if is_leaf_node {
                // We reached a leaf node - find partition point
                // The partition point is where keys transition from < to >=

                for pos in node_index..end {
                    let node_item = &self.node_items[pos];

                    // If we find a key greater than or equal to the search key,
                    // this is our partition point
                    if node_item >= &search_entry {
                        return Ok(SearchResultItem {
                            offset: node_item.offset as usize,
                            index: pos - leaf_nodes_offset,
                        });
                    }
                }

                // If no partition found in this node (all keys are < search_key),
                // the partition would be at the next node if any
                if end < self.level_bounds[0].end {
                    // Get the first item of the next node if it exists
                    let next_node_index = end;
                    if next_node_index < self.level_bounds[0].end {
                        let next_node_item = &self.node_items[next_node_index];
                        return Ok(SearchResultItem {
                            offset: next_node_item.offset as usize,
                            index: next_node_index - leaf_nodes_offset,
                        });
                    }
                }

                // If we reach here, it means all keys are less than the search key
                // and there's no next node. Return the last item as the partition point.
                if end > node_index {
                    let last_pos = end - 1;
                    let last_item = &self.node_items[last_pos];
                    return Ok(SearchResultItem {
                        offset: last_item.offset as usize,
                        index: last_pos - leaf_nodes_offset,
                    });
                }

                // This should never happen in a valid B-tree
                return Err(Error::Other(
                    "No partition point found in B-tree".to_string(),
                ));
            } else {
                // For internal nodes, we want to find the child that would contain
                // the partition point. We need to traverse to the rightmost child
                // that has a key <= search_key

                // Default to leftmost child
                let mut chosen_child_pos = node_index;

                // Find rightmost child with key <= search_key
                for pos in node_index..end {
                    let node_item = &self.node_items[pos];
                    if node_item.key <= search_entry.key {
                        chosen_child_pos = pos;
                    } else {
                        // Once we find a key > search_key, we've gone too far
                        break;
                    }
                }

                // Follow the chosen child
                queue.push_back((self.node_items[chosen_child_pos].offset as usize, level - 1));
            }
        }

        // This should never happen in a valid B-tree
        Err(Error::Other(
            "Failed to find partition point in B-tree".to_string(),
        ))
    }

    pub fn find_range(&self, lower: K, upper: K) -> Result<Vec<SearchResultItem>, Error> {
        let mut results = Vec::new();
        let lower_bound = self.find_partition(lower)?;
        let upper_bound = self.find_partition(upper)?;

        // The result will be items between the lower and upper bounds
        let leaf_nodes_offset = self
            .level_bounds
            .first()
            .expect("Btree has at least one level when node_size >= 2 and num_items > 0")
            .start;

        // Get the starting and ending positions in the leaf nodes
        let start_pos = lower_bound.index + leaf_nodes_offset;
        let end_pos = upper_bound.index + leaf_nodes_offset;

        // Collect all items in range
        for pos in start_pos..end_pos {
            let node_item = &self.node_items[pos];
            results.push(SearchResultItem {
                offset: node_item.offset as usize,
                index: pos - leaf_nodes_offset,
            });
        }

        Ok(results)
    }

    pub fn stream_find_exact<R: Read + Seek>(
        data: &mut R,
        num_items: usize,
        node_size: u16,
        key: K,
    ) -> Result<Vec<SearchResultItem>, Error> {
        let search_entry = NodeItem::new_with_key(key);
        let level_bounds = Stree::<K>::generate_level_bounds(num_items, node_size);
        let Range {
            start: leaf_nodes_offset,
            end: num_nodes,
        } = level_bounds
            .first()
            .expect("Btree has at least one level when node_size >= 2 and num_items > 0");

        // current position must be start of index
        let index_base = data.stream_position()?;

        // use ordered search queue to make index traversal in sequential order
        let mut queue = VecDeque::new();
        queue.push_back((0, level_bounds.len() - 1));
        let mut results = Vec::new();

        // Track visited leaf nodes to avoid duplicates when checking neighbors
        let mut visited_leaf_nodes = std::collections::HashSet::new();

        while let Some(next) = queue.pop_front() {
            let node_index = next.0;
            let level = next.1;
            // println!("popped next node_index: {node_index}, level: {level}");
            let is_leaf_node = node_index >= num_nodes - num_items;

            // Skip if we've already visited this leaf node
            if is_leaf_node && !visited_leaf_nodes.insert(node_index) {
                continue;
            }

            // find the end index of the node
            let end = min(node_index + node_size as usize, level_bounds[level].end);
            let length = end - node_index;
            let node_items = read_node_items(data, index_base, node_index, length)?;

            // Track if we found a match in this node and their positions
            let mut found_match = false;
            let mut match_positions = Vec::new();

            if is_leaf_node {
                // For leaf nodes, we're looking for exact matches
                for pos in node_index..end {
                    let node_pos = pos - node_index;
                    let node_item = &node_items[node_pos];
                    if search_entry.equals(node_item) {
                        found_match = true;
                        match_positions.push(pos);

                        let index = pos - leaf_nodes_offset;
                        let offset = node_item.offset as usize;
                        // println!("pushing leaf node. index: {index}, offset: {offset}");
                        results.push(SearchResultItem { offset, index });
                    }
                }

                // If we found a match, check neighboring nodes
                if found_match {
                    // Check if leftmost match is at the start of the node
                    // If so, check the previous node for matches at the end
                    if match_positions.first() == Some(&node_index)
                        && node_index > level_bounds[0].start
                    {
                        let prev_node_index = node_index - node_size as usize;
                        if !visited_leaf_nodes.contains(&prev_node_index) {
                            queue.push_back((prev_node_index, level));
                        }
                    }

                    // Check if rightmost match is at the end of the node
                    // If so, check the next node for matches at the beginning
                    if match_positions.last() == Some(&(end - 1)) && end < level_bounds[0].end {
                        let next_node_index = end;
                        if !visited_leaf_nodes.contains(&next_node_index) {
                            queue.push_back((next_node_index, level));
                        }
                    }
                }
            } else {
                // For internal nodes, find the appropriate child node(s) to traverse
                // Default to leftmost child
                let mut chosen_child_pos = 0; // Relative to node_items array
                let mut found_potential_path = false;

                // Find all children that could contain the search key
                for node_pos in 0..node_items.len() {
                    let node_item = &node_items[node_pos];
                    let actual_pos = node_index + node_pos;

                    // If we find an exact match in an internal node, we need to check:
                    // 1. The child node pointed to by this internal node
                    // 2. Also potentially the child node of the next internal node
                    if search_entry.equals(node_item) {
                        found_potential_path = true;
                        found_match = true;

                        // Add this child's subtree to the search queue
                        let offset = node_item.offset as usize;
                        let prev_level = level - 1;
                        queue.push_back((offset, prev_level));

                        // If this is not the last entry in the node, we might need to check
                        // the next child's subtree as well (depends on the B-tree implementation)
                        if node_pos + 1 < node_items.len() {
                            let next_node_item = &node_items[node_pos + 1];
                            queue.push_back((next_node_item.offset as usize, prev_level));
                        }
                    }
                    // For keys less than the search key, update the potential path
                    else if node_item.key < search_entry.key {
                        chosen_child_pos = node_pos;
                        found_potential_path = true;
                    }
                    // Once we find a key > search key, we've found the boundary
                    else {
                        // If we're at the first item and it's already > search_key,
                        // we need to go to this child
                        if node_pos == 0 {
                            chosen_child_pos = 0;
                            found_potential_path = true;
                        }
                        break;
                    }
                }

                // If we didn't find an exact match but found a potential path,
                // follow the chosen child
                if !found_potential_path {
                    // If all keys in the node are < search_key, go to the rightmost child
                    chosen_child_pos = node_items.len() - 1;
                    let offset = node_items[chosen_child_pos].offset as usize;
                    let prev_level = level - 1;
                    queue.push_back((offset, prev_level));
                } else if !found_match {
                    // This handles the case where we found a child to traverse but no exact match
                    let offset = node_items[chosen_child_pos].offset as usize;
                    let prev_level = level - 1;
                    queue.push_back((offset, prev_level));
                }
            }
        }

        // Skip rest of index
        data.seek(SeekFrom::Start(
            index_base + (num_nodes * Entry::<K>::SERIALIZED_SIZE) as u64,
        ))?;
        Ok(results)
    }

    // #[cfg(feature = "http")]
    // #[allow(clippy::too_many_arguments)]
    // pub async fn http_stream_search<T: AsyncHttpRangeClient>(
    //     client: &mut AsyncBufferedHttpRangeClient<T>,
    //     index_begin: usize,
    //     attr_index_size: usize,
    //     num_items: usize,
    //     branching_factor: u16,
    //     min_x: f64,
    //     min_y: f64,
    //     max_x: f64,
    //     max_y: f64,
    //     combine_request_threshold: usize,
    // ) -> Result<Vec<HttpSearchResultItem>, Error> {
    //     use tracing::debug;

    //     let bounds = NodeItem::bounds(min_x, min_y, max_x, max_y);
    //     if num_items == 0 {
    //         return Ok(vec![]);
    //     }
    //     let level_bounds = Stree::generate_level_bounds(num_items, branching_factor);
    //     let feature_begin =
    //         index_begin + attr_index_size + Stree::index_size(num_items, branching_factor);
    //     debug!("http_stream_search - index_begin: {index_begin}, feature_begin: {feature_begin} num_items: {num_items}, branching_factor: {branching_factor}, level_bounds: {level_bounds:?}, GPS bounds:[({min_x}, {min_y}), ({max_x},{max_y})]");

    //     #[derive(Debug, PartialEq, Eq)]
    //     struct NodeRange {
    //         level: usize,
    //         nodes: Range<usize>,
    //     }

    //     let mut queue = VecDeque::new();
    //     queue.push_back(NodeRange {
    //         nodes: 0..1,
    //         level: level_bounds.len() - 1,
    //     });
    //     let mut results = Vec::new();

    //     while let Some(node_range) = queue.pop_front() {
    //         debug!("next: {node_range:?}. {} items left in queue", queue.len());
    //         let node_items = read_http_node_items(client, index_begin, &node_range.nodes).await?;
    //         for (node_pos, node_item) in node_items.iter().enumerate() {
    //             if !bounds.intersects(node_item) {
    //                 continue;
    //             }

    //             if node_range.level == 0 {
    //                 // leaf node
    //                 let start = feature_begin + node_item.offset as usize;
    //                 if let Some(next_node_item) = &node_items.get(node_pos + 1) {
    //                     let end = feature_begin + next_node_item.offset as usize;
    //                     results.push(HttpSearchResultItem {
    //                         range: HttpRange::Range(start..end),
    //                     });
    //                 } else {
    //                     debug_assert_eq!(node_pos, num_items - 1);
    //                     results.push(HttpSearchResultItem {
    //                         range: HttpRange::RangeFrom(start..),
    //                     });
    //                 }
    //             } else {
    //                 let children_level = node_range.level - 1;
    //                 let mut children_nodes = node_item.offset as usize
    //                     ..(node_item.offset + branching_factor as u64) as usize;
    //                 if children_level == 0 {
    //                     // These children are leaf nodes.
    //                     //
    //                     // We can right-size our feature requests if we know the size of each feature.
    //                     //
    //                     // To infer the length of *this* feature, we need the start of the *next*
    //                     // feature, so we get an extra node here.
    //                     children_nodes.end += 1;
    //                 }
    //                 // always stay within level's bounds
    //                 children_nodes.end = min(children_nodes.end, level_bounds[children_level].end);

    //                 let children_range = NodeRange {
    //                     nodes: children_nodes,
    //                     level: children_level,
    //                 };

    //                 let Some(tail) = queue.back_mut() else {
    //                     debug!("Adding new request onto empty queue: {children_range:?}");
    //                     queue.push_back(children_range);
    //                     continue;
    //                 };

    //                 if tail.level != children_level {
    //                     debug!("Adding new request for new level: {children_range:?} (existing queue tail: {tail:?})");
    //                     queue.push_back(children_range);
    //                     continue;
    //                 }

    //                 let wasted_bytes = {
    //                     if children_range.nodes.start >= tail.nodes.end {
    //                         (children_range.nodes.start - tail.nodes.end) * size_of::<NodeItem>()
    //                     } else {
    //                         // To compute feature size, we fetch an extra leaf node, but computing
    //                         // wasted_bytes for adjacent ranges will overflow in that case, so
    //                         // we skip that computation.
    //                         //
    //                         // But let's make sure we're in the state we think we are:
    //                         debug_assert_eq!(
    //                             children_range.nodes.start + 1,
    //                             tail.nodes.end,
    //                             "we only ever fetch one extra node"
    //                         );
    //                         debug_assert_eq!(
    //                             children_level, 0,
    //                             "extra node fetching only happens with leaf nodes"
    //                         );
    //                         0
    //                     }
    //                 };
    //                 if wasted_bytes > combine_request_threshold {
    //                     debug!("Adding new request for: {children_range:?} rather than merging with distant NodeRange: {tail:?} (would waste {wasted_bytes} bytes)");
    //                     queue.push_back(children_range);
    //                     continue;
    //                 }

    //                 // Merge the ranges to avoid an extra request
    //                 debug!("Extending existing request {tail:?} with nearby children: {:?} (wastes {wasted_bytes} bytes)", &children_range.nodes);
    //                 tail.nodes.end = children_range.nodes.end;
    //             }
    //         }
    //     }
    //     Ok(results)
    // }

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
        // let list = tree.find_exact(0)?;
        // assert_eq!(list.len(), 1);
        // assert!(nodes[list[0].index].key == 0);

        let list = tree.find_exact(2)?;
        assert_eq!(list.len(), 1);
        assert!(nodes[list[0].index].key == 2);

        let list = tree.find_range(0, 2)?;
        assert_eq!(list.len(), 2);
        assert!(nodes[list[0].index].key == 0);
        assert!(nodes[list[1].index].key == 2);

        let list = tree.find_range(1, 3)?;
        assert_eq!(list.len(), 2);
        assert!(nodes[list[0].index].key == 1);
        assert!(nodes[list[1].index].key == 2);

        let list = tree.find_range(3, 4)?;
        assert_eq!(list.len(), 0);
        Ok(())
    }

    #[test]
    fn tree_19items_roundtrip_stream_search() -> Result<()> {
        let mut nodes = vec![
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
            // NodeItem::new(18_u64, 18_u64),
        ];

        let mut offset = 0;
        for node in &mut nodes {
            node.offset = offset;
            offset += NodeItem::<u64>::SERIALIZED_SIZE as u64;
        }
        let tree = Stree::build(&nodes, 3)?;
        let list = tree.find_exact(10)?;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].index, 10);

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

    // #[test]
    // fn tree_100_000_items_in_denmark() -> Result<()> {
    //     use rand::distributions::{Distribution, Uniform};

    //     let unifx = Uniform::from(466379..708929);
    //     let unify = Uniform::from(6096801..6322352);
    //     let mut rng = rand::thread_rng();

    //     let mut nodes = Vec::new();
    //     for _ in 0..100000 {
    //         let x = unifx.sample(&mut rng) as f64;
    //         let y = unify.sample(&mut rng) as f64;
    //         nodes.push(NodeItem::bounds(x, y, x, y));
    //     }

    //     let extent = calc_extent(&nodes);
    //     hilbert_sort(&mut nodes, &extent);
    //     let tree = Stree::build(&nodes, &extent, Stree::DEFAULT_NODE_SIZE)?;
    //     let list = tree.search(690407.0, 6063692.0, 811682.0, 6176467.0)?;

    //     for i in 0..list.len() {
    //         assert!(nodes[list[i].index]
    //             .intersects(&NodeItem::bounds(690407.0, 6063692.0, 811682.0, 6176467.0)));
    //     }

    //     let mut tree_data: Vec<u8> = Vec::new();
    //     let res = tree.stream_write(&mut tree_data);
    //     assert!(res.is_ok());

    //     let mut reader = Cursor::new(&tree_data);
    //     let list2 = Stree::stream_search(
    //         &mut reader,
    //         nodes.len(),
    //         Stree::DEFAULT_NODE_SIZE,
    //         690407.0,
    //         6063692.0,
    //         811682.0,
    //         6176467.0,
    //     )?;
    //     assert_eq!(list2.len(), list.len());
    //     for i in 0..list2.len() {
    //         assert!(nodes[list2[i].index]
    //             .intersects(&NodeItem::bounds(690407.0, 6063692.0, 811682.0, 6176467.0)));
    //     }
    //     Ok(())
    // }
}
