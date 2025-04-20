// use crate::entry::{Entry, Offset};
// use crate::error::Error;
// use crate::key::Key;
// use std::io::{Read, Seek};
// use std::marker::PhantomData;
// use std::mem;

// /// Represents the static B+Tree structure, providing read access.
// /// `K` is the Key type, `R` is the underlying readable and seekable data source.
// #[derive(Debug)]
// pub struct StaticBTree<K: Key, R: Read + Seek> {
//     /// The underlying data source (e.g., file, memory buffer).
//     reader: R,
//     /// The branching factor B (number of keys/entries per node). Fixed at creation.
//     branching_factor: u16,
//     /// Total number of key-value entries stored in the tree.
//     num_entries: u64,
//     /// Height of the tree. 0 for empty, 1 for root-only leaf, etc.
//     height: u8,
//     /// The size of the header section in bytes at the beginning of the data source.
//     entry_size: usize,

//     nodes: Vec<Vec<Entry<K>>>,
//     /// Marker for the generic Key type.
//     _phantom_key: PhantomData<K>,
// }

// impl<K: Key, R: Read + Seek> StaticBTree<K, R> {
//     pub fn from_reader(reader: R, branching_factor: u16, num_entries: u64) -> Result<Self, Error> {
//         let height = 0; // TODO: height can be derived from the number of entries and branching factor

//         let key_size = K::SERIALIZED_SIZE;
//         let offset_size = mem::size_of::<Offset>();
//         let entry_size = key_size + offset_size;

//         Ok(StaticBTree::<K, R> {
//             reader,
//             branching_factor,
//             num_entries,
//             height,
//             entry_size,
//             nodes: Vec::new(),
//             _phantom_key: PhantomData,
//         })
//     }

//     /// This will be implemented in the future.
//     // pub fn from_http_reader<T: AsyncHttpRangeClient>(
//     //     client: &mut AsyncBufferedHttpRangeClient<T>,
//     //     index_begin: usize,
//     //     num_entries: usize,
//     //     branching_factor: u16,
//     // ) -> Result<Self, Error> {

//     // }

//     /// Finds the value associated with a given key.

//     pub fn find(&mut self, search_key: &K) -> Result<Option<Vec<Value>>, Error> {
//         if self.height == 0 {
//             println!("find: empty tree");
//             return Ok(None);
//         }

//         // find lower_bound of search_key in the root node
//         // Read bytes corresponding to the root node and read children nodes recursively when it's necessary.
//         // Here we can use `read_from` of Entry struct to read the node.
//         // To avoid reading big amount of data, this will read only the necessary nodes. To optimize the performance, prefetch the nodes when it's necessary.

//         // if it finds the key, check the neighboring keys since there might be duplicates. If the found key is the first or last, check the next or previous node respectively.

//         // TODO: implement
//         Err(Error::QueryError("not implemented".to_string()))
//     }

//     // --- range ---
//     pub fn range(&mut self, min_key: &K, max_key: &K) -> Result<Vec<Value>, Error> {
//         // TODO: implement
//         Err(Error::QueryError("not implemented".to_string()))

//         // find the lower_bound of min_key in the root node

//         // find the upper_bound of max_key in the root node

//         // iterate through the nodes between the lower_bound and upper_bound
//     }

//     fn prefetch_nodes(&mut self, node_index: usize) -> Result<(), Error> {
//         // read the node from the reader and store it in the nodes vector. This is to optimize the performance.
//         // TODO: implement
//         Err(Error::QueryError("not implemented".to_string()))
//     }

//     // --- Accessors ---
//     pub fn branching_factor(&self) -> u16 {
//         self.branching_factor
//     }
//     pub fn len(&self) -> u64 {
//         self.num_entries
//     }
//     pub fn is_empty(&self) -> bool {
//         self.num_entries == 0
//     }
//     pub fn height(&self) -> u8 {
//         self.height
//     }
// }

// #[cfg(test)]
// mod tests {
//     use super::*;

//     // test1: test with a simple example such as u64 key and 3 items and find the key.

//     // test2: test with a simple example such as u64 key and 3 items and range query.

//     // test3: test with more entries such as 20 with branching factor 3 and find the key.

//     // test4: test with more entries such as 20 with branching factor 3 and range query.

//     // test5: test with other types such as f32 and string and find the key.

//     // test6: test with other types such as f32 and string and range query.
// }
