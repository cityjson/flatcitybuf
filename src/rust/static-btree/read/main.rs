use std::{
    fs::File,
    io::{Cursor, Write},
};

use static_btree::{StaticBTree, StaticBTreeBuilder};

/// Test keys: 10 evenly spaced entries in [0..100_000).
const TEST_KEYS: [u32; 10] = [
    0, 10000, 20000, 30000, 40000, 50000, 60000, 70000, 80000, 90000,
];

/// Prepare a test tree: 10k unique keys (10k entries).
fn prepare_tree() -> StaticBTree<u32, File> {
    let bf = 32;
    let entries = 100_000;
    // let mut builder = StaticBTreeBuilder::new(bf);
    // for k in 0..entries {
    //     builder.push(k as u32, k as u64);
    // }
    // println!("building tree");
    // let data = builder.build().expect("build tree");

    // // Persist to temporary file
    // let mut file = File::create("test.btree").expect("create file");
    // file.write_all(&data).expect("write data");
    // let cursor = Cursor::new(data);
    let file = File::open("test.btree").expect("open file");
    StaticBTree::new(file, bf, entries as u64).expect("open tree failed")
}

fn bench_find_eq() {
    // let mut tree = prepare_tree();
    // for &key in &TEST_KEYS {
    //     let v = tree.find_eq(&key).unwrap();

    //     println!("key: {}, value: {:?}", key, v);
    // }
}

fn main() {
    bench_find_eq();
}
