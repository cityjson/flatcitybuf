use crate::entry::{Entry, Offset};
use crate::error::Error;
use crate::key::Key;
use std::io::{Seek, Write};
use std::marker::PhantomData;
use std::mem;

#[cfg(test)]
mod tests {
    use super::*;

    // test1: test with a simple example such as u64 key and 3 items.

    // test2: test with more entries such as 20 with branching factor 3.

    // test with other types such as f32 and string
}
