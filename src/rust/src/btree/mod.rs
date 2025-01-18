use anyhow::Result;

use serde_json::Value;
use std::{
    cmp::{max, min},
    io::{Read, Write},
    ops::Range,
};

#[derive(Clone, PartialEq, Debug)]
pub struct NodeItem {
    pub val: Value,
    pub offset: u64,
}

impl NodeItem {
    pub fn new(val: Value, offset: u64) -> NodeItem {
        todo!("implement me")
    }

    pub fn from_reader(mut rdr: impl Read) -> Result<Self> {
        todo!("implement me")
    }

    pub fn from_bytes(raw: &[u8]) -> Result<Self> {
        todo!("implement me")
    }

    pub fn write<W: Write>(&self, wtr: &mut W) -> std::io::Result<()> {
        todo!("implement me")
    }

    pub fn set_val(&mut self, val: Value) {
        self.val = val;
    }

    pub fn set_offset(&mut self, offset: u64) {
        self.offset = offset;
    }
}

fn read_node_vec(node_items: &mut Vec<NodeItem>, mut data: impl Read) -> Result<()> {
    node_items.clear();
    for _ in 0..node_items.capacity() {
        node_items.push(NodeItem::from_reader(&mut data)?);
    }
    Ok(())
}

fn read_node_items<R: Read + Seek>(
    data: &mut R,
    base: u64,
    node_index: usize,
    length: usize,
) -> Result<Vec<NodeItem>> {
    todo!("implement me")
}

#[derive(Debug)]
pub struct SearchResultItem {
    pub offset: usize,
    pub index: usize,
}

pub struct BTree {
    // TODO: add min and max values of the field
    node_items: Vec<NodeItem>,
    num_leaf_nodes: usize,
    branching_factor: u16,
    level_bounds: Vec<Range<usize>>,
}

impl BTree {
    pub const DEFAULT_NODE_SIZE: u16 = 16;

    fn init(&mut self, node_size: u16) -> Result<()> {
        assert!(node_size >= 2, "Node size must be at least 2");
        assert!(self.num_leaf_nodes > 0, "Cannot create empty tree");
        self.branching_factor = min(max(node_size, 2u16), 65535u16);
        self.level_bounds =
            BTree::generate_level_bounds(self.num_leaf_nodes, self.branching_factor);
        let num_nodes = self
            .level_bounds
            .first()
            .expect("RTree has at least one level when node_size >= 2 and num_items > 0")
            .end;
        self.node_items = vec![NodeItem::create(0); num_nodes];
        Ok(())
    }

    fn generate_level_bounds(num_items: usize, node_size: u16) -> Vec<Range<usize>> {
        todo!("implement me")
    }

    fn generate_nodes(&mut self) {
        todo!("implement me")
    }

    fn read_data(&mut self, data: impl Read) -> Result<()> {
        todo!("implement me")
    }

    fn num_nodes(&self) -> usize {
        self.node_items.len()
    }

    pub fn build(nodes: &Vec<NodeItem>, node_size: u16) -> Result<Self> {
        todo!("implement me")
    }

    pub fn from_buf(data: impl Read, num_items: usize, node_size: u16) -> Result<Self> {
        todo!("implement me")
    }

    pub fn search(&self, query: &[u8]) -> Result<Vec<SearchResultItem>> {
        todo!("implement me")
    }
}
