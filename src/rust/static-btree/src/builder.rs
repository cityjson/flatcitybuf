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
                offset: 0,
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
        let dups = tree.lower_bound(&10).unwrap();
        assert_eq!(dups, vec![100]);
    }
}
