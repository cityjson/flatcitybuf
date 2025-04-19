use crate::entry::Offset;
use crate::error::Error;
use crate::key::Key;
use crate::tree::StaticBTree;
use std::io::{Read, Seek};

/// Comparison operators supported by StaticBTree queries.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Comparison {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
}

/*
impl<K: Key, R: Read + Seek> StaticBTree<K, R> {
    // Query methods (planned, not yet public)
}
*/
