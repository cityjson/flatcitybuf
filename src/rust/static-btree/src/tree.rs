use crate::entry::{Entry, Offset};
use crate::error::Error;
use crate::key::Key;
use std::io::{Read, Seek};
use std::marker::PhantomData;
use std::mem;

// #[cfg(test)]
// mod tests {
//     use super::*;

// test1: test with a simple example such as u64 key and 3 items and find the key.

// test2: test with a simple example such as u64 key and 3 items and range query.

// test3: test with more entries such as 20 with branching factor 3 and find the key.

// test4: test with more entries such as 20 with branching factor 3 and range query.

// test5: test with other types such as f32 and string and find the key.

// test6: test with other types such as f32 and string and range query.
// }
