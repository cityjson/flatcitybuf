use super::tree::Layout;
use crate::entry::{Entry, Offset};
use crate::error::Error;
use crate::key::Key;
use std::io::Write;

/// Builder for a serialized static B+Tree.
/// Collect entries in **sorted order** and call `build()` to obtain a ready‑to‑store byte vector.
///
/// Duplicate keys are allowed.
pub struct StaticBTreeBuilder<K: Key> {
    branching_factor: usize,
    entries: Vec<Entry<K>>, // leaf entries only
}

impl<K: Key> StaticBTreeBuilder<K> {
    pub fn new(branching_factor: u16) -> Self {
        Self {
            branching_factor: branching_factor as usize,
            entries: Vec::new(),
        }
    }

    pub fn push(&mut self, key: K, offset: Offset) {
        self.entries.push(Entry { key, offset });
    }

    /// Consume builder and return serialized byte vector.
    pub fn build(mut self) -> Result<Vec<u8>, Error> {
        // Ensure entries are sorted
        if !self.entries.windows(2).all(|w| w[0].key <= w[1].key) {
            return Err(Error::BuildError("entries must be sorted".into()));
        }

        let b = self.branching_factor;
        let num_entries = self.entries.len();
        let layout = Layout::new(num_entries, b);

        // Prepare layers: root..leaf order vector of vec<Entry<K>>
        let mut layers: Vec<Vec<Entry<K>>> = Vec::new();

        // 1. Leaf layer with padding
        let mut leaf_layer = self.entries;
        Self::pad_layer(&mut leaf_layer, b);
        layers.push(leaf_layer.clone());

        // 2. Build internal layers bottom‑up
        let mut prev = leaf_layer;
        while prev.len() > b {
            let mut next_layer = Vec::new();
            for chunk_start in (0..prev.len()).step_by(b) {
                // let right_pos = chunk_start + b;
                // let key = if chunk < prev.len() {
                //     prev[right_pos].key.clone()
                // } else {
                //     prev.last().unwrap().key.clone()
                // };
                let key = prev[chunk_start].key.clone();
                next_layer.push(Entry { key, offset: 0 });
            }
            Self::pad_layer(&mut next_layer, b);
            layers.push(next_layer.clone());
            prev = next_layer;
        }
        layers.reverse(); // root first

        // Serialize
        let mut buf: Vec<u8> =
            Vec::with_capacity((layout.last_entry_index() + b) * (Entry::<K>::SERIALIZED_SIZE));
        for layer in layers {
            for entry in layer {
                entry.write_to(&mut buf)?;
            }
        }
        Ok(buf)
    }

    fn pad_layer(layer: &mut Vec<Entry<K>>, b: usize) {
        if layer.is_empty() {
            return;
        }
        let pad_needed = (b - (layer.len() % b)) % b;
        let last = layer.last().unwrap().clone();
        for _ in 0..pad_needed {
            layer.push(Entry {
                key: last.key.clone(),
                offset: last.offset,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::StaticBTree;
    use std::io::Cursor;

    #[test]
    fn builder_roundtrip() {
        let mut builder = StaticBTreeBuilder::new(4);
        let data = vec![(10, 100), (15, 150), (20, 201), (20, 202), (25, 250)];
        for (k, off) in &data {
            builder.push(*k, *off);
        }
        let bytes = builder.build().unwrap();
        let mut tree: StaticBTree<i32, _> =
            StaticBTree::new(Cursor::new(bytes), 4, data.len() as u64).unwrap();
        // exact match on first key
        assert_eq!(tree.lower_bound(&10).unwrap(), vec![100]);
    }
    // Unsorted entries should produce a build error
    #[test]
    fn builder_unsorted_error() {
        let mut builder = StaticBTreeBuilder::new(4);
        builder.push(5, 50);
        builder.push(3, 30);
        assert!(builder.build().is_err());
    }
    // Single-entry tree: height=1, all lookups return the only offset
    #[test]
    fn builder_single_entry() {
        let mut builder = StaticBTreeBuilder::new(4);
        builder.push(42, 420);
        let bytes = builder.build().unwrap();
        let mut tree: StaticBTree<i32, _> = StaticBTree::new(Cursor::new(bytes), 4, 1).unwrap();
        assert_eq!(tree.height(), 1);
        assert_eq!(tree.lower_bound(&42).unwrap(), vec![420]);
        assert_eq!(tree.lower_bound(&0).unwrap(), vec![420]);
    }
    // Multi-layer integer tree with branching factor 3
    #[test]
    fn builder_multilayer_integer() {
        let data: Vec<(i32, u64)> = (1..=10).map(|k| (k, k as u64 * 10)).collect();
        let mut builder = StaticBTreeBuilder::new(3);
        for (k, off) in &data {
            builder.push(*k, *off);
        }
        let bytes = builder.build().unwrap();
        let mut tree: StaticBTree<i32, _> =
            StaticBTree::new(Cursor::new(bytes), 3, data.len() as u64).unwrap();
        // tree should have multiple layers
        assert!(tree.height() > 1);
        // only verify height for multi-layer setup

        // exact match on first key
        assert_eq!(tree.lower_bound(&1).unwrap(), vec![10]);
        // lower_bound between keys
        assert_eq!(tree.lower_bound(&5).unwrap(), vec![50]);
        // lower_bound before first key
        assert_eq!(tree.lower_bound(&0).unwrap(), vec![10]);
    }
    // Duplicate keys that span padding boundaries
    #[test]
    fn builder_duplicate_padding() {
        // use leaf-only tree to avoid internal layers
        let data = vec![(10, 1), (20, 2), (20, 3), (20, 4), (20, 5), (30, 6)];
        let mut builder = StaticBTreeBuilder::new(3);
        for (k, off) in &data {
            builder.push(*k, *off);
        }
        let bytes = builder.build().unwrap();
        let mut tree: StaticBTree<i32, _> =
            StaticBTree::new(Cursor::new(bytes), 3, data.len() as u64).unwrap();
        assert_eq!(tree.lower_bound(&20).unwrap(), vec![2, 3, 4, 5]);
    }
    // Floating-point keys and duplicates
    #[test]
    fn builder_float_keys() {
        use ordered_float::OrderedFloat;
        // use leaf-only tree: branching factor >= number of entries to force single layer
        let data = vec![
            (OrderedFloat(0.1f32), 1),
            (OrderedFloat(0.2f32), 2),
            (OrderedFloat(0.2f32), 3),
            (OrderedFloat(0.3f32), 4),
            (OrderedFloat(0.5f32), 5),
        ];
        let mut builder = StaticBTreeBuilder::new(3);
        for (k, off) in &data {
            builder.push(*k, *off);
        }
        let bytes = builder.build().unwrap();
        let mut tree: StaticBTree<OrderedFloat<f32>, _> =
            StaticBTree::new(Cursor::new(bytes), 3, data.len() as u64).unwrap();
        assert_eq!(tree.lower_bound(&OrderedFloat(0.2f32)).unwrap(), vec![2, 3]);
        assert_eq!(tree.lower_bound(&OrderedFloat(0.25f32)).unwrap(), vec![4]);
    }
    // Fixed-string keys (length 4) and lexicographic ordering
    #[test]
    fn builder_string_keys() {
        use crate::key::FixedStringKey;
        // test sequence of fixed-size string keys
        let data = vec!["a", "ab", "ab", "b", "zz"];
        // use leaf-only tree: branching factor >= entries to force single layer
        let mut builder = StaticBTreeBuilder::new(3);
        for (i, s) in data.iter().enumerate() {
            let k = FixedStringKey::<4>::from_str(s);
            builder.push(k, i as u64);
        }
        let bytes = builder.build().unwrap();
        let mut tree: StaticBTree<FixedStringKey<4>, _> =
            StaticBTree::new(Cursor::new(bytes), 3, data.len() as u64).unwrap();
        assert_eq!(
            tree.lower_bound(&FixedStringKey::<4>::from_str("ab"))
                .unwrap(),
            vec![1, 2]
        );
        // "ba" falls between "b" and "zz", so first >= "ba" is "zz" at offset 4
        assert_eq!(
            tree.lower_bound(&FixedStringKey::<4>::from_str("ba"))
                .unwrap(),
            vec![4]
        );
    }
    // Height calculation for various sizes and branching-factor 3
    #[test]
    fn builder_height_calculation() {
        // N=4 -> height=2
        let mut bldr = StaticBTreeBuilder::new(3);
        for i in 0..4 {
            bldr.push(i, i as u64);
        }
        let bytes = bldr.build().unwrap();
        let tree: StaticBTree<i32, _> = StaticBTree::new(Cursor::new(bytes), 3, 4).unwrap();
        assert_eq!(tree.height(), 2);
        // N=9 -> height=2
        let mut bldr = StaticBTreeBuilder::new(3);
        for i in 0..9 {
            bldr.push(i, i as u64);
        }
        let bytes = bldr.build().unwrap();
        let tree: StaticBTree<i32, _> = StaticBTree::new(Cursor::new(bytes), 3, 9).unwrap();
        assert_eq!(tree.height(), 2);
        // N=10 -> height=3
        let mut bldr = StaticBTreeBuilder::new(3);
        for i in 0..10 {
            bldr.push(i, i as u64);
        }
        let bytes = bldr.build().unwrap();
        let tree: StaticBTree<i32, _> = StaticBTree::new(Cursor::new(bytes), 3, 10).unwrap();
        assert_eq!(tree.height(), 3);
    }
}
