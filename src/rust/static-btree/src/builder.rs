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
    // raw input pairs; duplicates allowed
    // To be grouped in build(): Vec<(K, Vec<Offset>)>
    entries: Vec<(K, Offset)>,
}

impl<K: Key> StaticBTreeBuilder<K> {
    pub fn new(branching_factor: u16) -> Self {
        // Initialize builder with target branching factor
        StaticBTreeBuilder {
            branching_factor: branching_factor as usize,
            entries: Vec::new(),
        }
    }

    pub fn push(&mut self, key: K, offset: Offset) {
        // TODO: collect (key, offset) pairs for grouping into payload blocks
        self.entries.push((key, offset));
    }

    /// Consume builder and return serialized byte vector.
    pub fn build(self) -> Result<Vec<u8>, Error> {
        let b = self.branching_factor;
        // Empty input => empty tree
        if self.entries.is_empty() {
            return Ok(Vec::new());
        }
        // Sort raw entries by key to group duplicates
        let mut pairs = self.entries;
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        // Group offsets under each unique key
        let mut groups: Vec<(K, Vec<Offset>)> = Vec::new();
        for (key, offset) in pairs {
            if let Some((last_key, vec)) = groups.last_mut() {
                if *last_key == key {
                    vec.push(offset);
                    continue;
                }
            }
            groups.push((key, vec![offset]));
        }
        let unique = groups.len();
        // Entry serialized size
        let entry_size = Entry::<K>::SERIALIZED_SIZE;
        // Compute padded layer counts (leaf and internal)
        let mut layer_counts = Vec::new();
        // Leaf layer: pad unique to multiple of b
        let mut count = unique.div_ceil(b) * b;
        layer_counts.push(count);
        // Internal layers until a layer fits in one node
        while count > b {
            let raw = count.div_ceil(b);
            count = raw.div_ceil(b) * b;
            layer_counts.push(count);
        }
        // Total index entries across all layers
        let total_index_entries: usize = layer_counts.iter().sum();
        // File offset where payload region begins
        let payload_start = (total_index_entries * entry_size) as u64;
        // Build payload blocks and record first-block pointers
        let mut payload_buf: Vec<u8> = Vec::new();
        let mut first_ptrs = Vec::with_capacity(unique);
        for (_key, offsets) in &groups {
            let mut chunks = offsets.chunks(b).peekable();
            let mut first_ptr: u64 = 0;
            while let Some(chunk) = chunks.next() {
                let block_offset = payload_start + payload_buf.len() as u64;
                if first_ptr == 0 {
                    first_ptr = block_offset;
                }
                // next_ptr points to next block or 0
                let next_ptr = if chunks.peek().is_some() {
                    let block_size = 4u64 + 8u64 + (b as u64) * 8u64;
                    block_offset + block_size
                } else {
                    0u64
                };
                // write count (u32)
                payload_buf.extend(&(chunk.len() as u32).to_le_bytes());
                // write next_ptr (u64)
                payload_buf.extend(&next_ptr.to_le_bytes());
                // write offsets and pad to capacity b
                for &off in chunk {
                    payload_buf.extend(&off.to_le_bytes());
                }
                for _ in 0..(b - chunk.len()) {
                    payload_buf.extend(&0u64.to_le_bytes());
                }
            }
            first_ptrs.push(first_ptr);
        }
        // Build leaf layer entries (pad to layer_counts[0])
        let mut leaf_entries: Vec<Entry<K>> = groups
            .into_iter()
            .zip(first_ptrs.into_iter())
            .map(|((key, _), ptr)| Entry::new(key, ptr))
            .collect();
        if let Some(last) = leaf_entries.last().cloned() {
            leaf_entries.resize(layer_counts[0], last);
        }
        // Build internal layers (bottom-up)
        let mut layers: Vec<Vec<Entry<K>>> = Vec::new();
        layers.push(leaf_entries);
        for &lc in layer_counts.iter().skip(1) {
            let prev = layers.last().unwrap();
            let mut parent = Vec::new();
            for chunk in prev.chunks(b) {
                parent.push(chunk.last().unwrap().clone());
            }
            // pad to lc
            if let Some(last) = parent.last().cloned() {
                parent.resize(lc, last);
            }
            layers.push(parent);
        }

        // Serialize index region entries (root-to-leaf)
        let mut buf = Vec::with_capacity(total_index_entries * entry_size + payload_buf.len());
        // println!("layers num: {:?}", layers.len());
        for layer in layers.iter().rev() {
            // println!("layer: {:?}", layer);
            for entry in layer {
                entry.write_to(&mut buf)?;
            }
        }
        // Append payload blocks
        buf.extend(payload_buf);
        Ok(buf)
    }

    // fn pad_layer(...) – no longer needed; payload blocks handle duplicate distribution
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::StaticBTree;
    use std::io::Cursor;
}
