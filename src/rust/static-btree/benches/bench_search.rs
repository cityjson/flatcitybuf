use bst::{BufferedIndex, KeyValue as BstKeyValue, TypedSearchableIndex, IndexSerializable};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use static_btree::{StaticBTree, StaticBTreeBuilder};
use std::io::Cursor;
use std::io::{Write, Seek, SeekFrom};
use tempfile::tempfile;

/// Test keys: 10 evenly spaced entries in [0..100_000).
const TEST_KEYS: [u32; 10] = [
    0, 10000, 20000, 30000, 40000, 50000, 60000, 70000, 80000, 90000,
];

/// Prepare a test tree: 10k unique keys (10k entries).
fn prepare_tree() -> StaticBTree<u32, Cursor<Vec<u8>>> {
    let bf = 64;
    let entries = 100_000;
    let mut builder = StaticBTreeBuilder::new(bf);
    for k in 0..entries {
        builder.push(k as u32, k as u64);
    }
    let data = builder.build().expect("build tree");
    let cursor = Cursor::new(data);
    StaticBTree::new(cursor, bf, entries as u64).expect("open tree failed")
}

fn bench_find_eq(c: &mut Criterion) {
    let mut tree = prepare_tree();
    c.bench_function("static_btree::find_eq (10 keys)", |b| {
        b.iter(|| {
            // search for each test key
            for &key in &TEST_KEYS {
                let v = tree.find_eq(black_box(&key)).unwrap();
                black_box(v.len());
            }
        })
    });
}

/// Benchmark find_eq from a disk-backed B+tree (read index from a temp file)
fn bench_find_eq_from_disk(c: &mut Criterion) {
    // Build and serialize tree data
    let bf = 64;
    let entries = 100_000;
    let mut builder = StaticBTreeBuilder::new(bf);
    for k in 0..entries {
        builder.push(k as u32, k as u64);
    }
    let data = builder.build().expect("build tree");

    // Persist to temporary file
    let mut file = tempfile().expect("create temp file");
    file.write_all(&data).expect("write data");
    file.seek(SeekFrom::Start(0)).expect("rewind");

    // Open B+tree over file
    let mut tree = StaticBTree::new(file, bf, entries as u64)
        .expect("open tree from disk failed");

    c.bench_function("static_btree::find_eq_from_disk (10 keys)", |b| {
        b.iter(|| {
            for &key in &TEST_KEYS {
                let v = tree.find_eq(black_box(&key)).unwrap();
                black_box(v.len());
            }
        })
    });
}

fn bench_find_range(c: &mut Criterion) {
    let mut tree = prepare_tree();
    let low = 4_900u32;
    let high = 5_100u32;
    c.bench_function("static_btree::find_range(lt..gt)", |b| {
        b.iter(|| {
            let mut v = Vec::new();
            v.extend(tree.find_lt(black_box(&high)).unwrap());
            v.extend(tree.find_gt(black_box(&low)).unwrap());
            black_box(v.len());
        })
    });
}

/// Compare with Rust's built-in Vec::binary_search on sorted data.
fn bench_vec_binary_search(c: &mut Criterion) {
    let vec: Vec<u32> = (0..100_000u32).collect();
    c.bench_function("vec::binary_search (10 keys)", |b| {
        b.iter(|| {
            // binary_search for each test key
            for &key in &TEST_KEYS {
                let idx = vec.binary_search(black_box(&key)).unwrap();
                black_box(idx);
            }
        })
    });
}

/// Benchmark BST crate's in-memory buffered index (exact match)
fn bench_bst_buffered_index(c: &mut Criterion) {
    // Prepare 10k unique keys with single offsets
    let mut entries: Vec<BstKeyValue<u32>> = Vec::with_capacity(100_000);
    for k in 0..100_000u32 {
        entries.push(BstKeyValue {
            key: k,
            offsets: vec![k as u64],
        });
    }
    let mut index = BufferedIndex::new();
    index.build_index(entries);
    c.bench_function("bst::buffered_index::query_exact (10 keys)", |b| {
        b.iter(|| {
            for &key in &TEST_KEYS {
                let res = index.query_exact(black_box(&key)).unwrap_or(&[]);
                black_box(res.len());
            }
        })
    });
}
/// Benchmark BST crate's buffered index read from disk
fn bench_bst_buffered_index_from_disk(c: &mut Criterion) {
    // Prepare and build index in memory
    let count = 100_000u32;
    let mut entries: Vec<BstKeyValue<u32>> = Vec::with_capacity(count as usize);
    for k in 0..count {
        entries.push(BstKeyValue { key: k, offsets: vec![k as u64] });
    }
    let mut idx = BufferedIndex::new();
    idx.build_index(entries);
    // Serialize to temp file
    let mut file = tempfile().expect("create temp file");
    idx.serialize(&mut file).expect("serialize index");
    file.seek(SeekFrom::Start(0)).expect("rewind file");
    // Deserialize from disk
    let index_on_disk = BufferedIndex::<u32>::deserialize(&mut file)
        .expect("deserialize index");
    c.bench_function("bst::buffered_index::query_exact_from_disk (10 keys)", |b| {
        b.iter(|| {
            for &key in &TEST_KEYS {
                let res = index_on_disk.query_exact(black_box(&key)).unwrap_or(&[]);
                black_box(res.len());
            }
        })
    });
}

criterion_group!(
    benches,
    bench_find_eq,
    bench_find_eq_from_disk,
    bench_find_range,
    bench_vec_binary_search,
    bench_bst_buffered_index,
    bench_bst_buffered_index_from_disk
);
criterion_main!(benches);
