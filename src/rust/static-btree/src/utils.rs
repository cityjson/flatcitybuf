// Utility functions for the static-btree implementation

/// Calculate the height of a static B+tree with the given parameters
///
/// # Parameters
///
/// * `total_entries` - The total number of entries in the tree
/// * `branching_factor` - The branching factor of the tree
///
/// # Returns
///
/// The height of the tree (1 for a single node, 2 for root + leaves, etc.)
pub fn calculate_tree_height(total_entries: usize, branching_factor: usize) -> usize {
    if total_entries == 0 {
        return 1; // Empty tree still has a root node
    }

    // Calculate how many entries fit at each level
    let mut height = 1; // Start with just a root node
    let mut level_capacity = branching_factor;

    while level_capacity < total_entries {
        height += 1;
        level_capacity *= branching_factor;
    }

    height
}

/// Calculate the total number of nodes needed for a static B+tree
///
/// # Parameters
///
/// * `total_entries` - The total number of entries in the tree
/// * `branching_factor` - The branching factor of the tree
///
/// # Returns
///
/// The total number of nodes needed for the tree
pub fn calculate_total_nodes(total_entries: usize, branching_factor: usize) -> usize {
    if total_entries == 0 {
        return 1; // Empty tree still has a root node
    }

    // For a perfect B+tree, the number of nodes is:
    // - For leaf level: ceil(total_entries / branching_factor)
    // - For each internal level: ceil(nodes_in_level_below / branching_factor)

    let leaf_nodes = div_ceil(total_entries, branching_factor);
    let mut total_nodes = leaf_nodes;
    let mut nodes_below = leaf_nodes;

    while nodes_below > 1 {
        let nodes_at_level = div_ceil(nodes_below, branching_factor);
        total_nodes += nodes_at_level;
        nodes_below = nodes_at_level;
    }

    total_nodes
}

/// Calculate ceiling of integer division
///
/// # Parameters
///
/// * `a` - The dividend
/// * `b` - The divisor
///
/// # Returns
///
/// The ceiling of a / b
#[inline]
pub fn div_ceil(a: usize, b: usize) -> usize {
    (a + b - 1) / b
}

/// Check if a value is aligned to a given boundary
///
/// # Parameters
///
/// * `value` - The value to check
/// * `alignment` - The alignment boundary
///
/// # Returns
///
/// True if value is aligned to the boundary
#[inline]
pub fn is_aligned(value: usize, alignment: usize) -> bool {
    value % alignment == 0
}

/// Align a value up to the next boundary
///
/// # Parameters
///
/// * `value` - The value to align
/// * `alignment` - The alignment boundary
///
/// # Returns
///
/// The value aligned up to the next boundary
#[inline]
pub fn align_up(value: usize, alignment: usize) -> usize {
    div_ceil(value, alignment) * alignment
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tree_height() {
        // Empty tree
        assert_eq!(calculate_tree_height(0, 16), 1);

        // Single node tree (fits in root)
        assert_eq!(calculate_tree_height(16, 16), 1);

        // Two-level tree
        assert_eq!(calculate_tree_height(17, 16), 2);
        assert_eq!(calculate_tree_height(256, 16), 2);

        // Three-level tree
        assert_eq!(calculate_tree_height(257, 16), 3);
    }

    #[test]
    fn test_total_nodes() {
        // Empty tree
        assert_eq!(calculate_total_nodes(0, 16), 1);

        // Single node tree
        assert_eq!(calculate_total_nodes(16, 16), 1);

        // Two-level tree:
        // - 17 entries require 2 leaf nodes + 1 root = 3 nodes
        assert_eq!(calculate_total_nodes(17, 16), 3);

        // - 256 entries require 16 leaf nodes + 1 root = 17 nodes
        assert_eq!(calculate_total_nodes(256, 16), 17);

        // Three-level tree:
        // - 257 entries require 17 leaf nodes + 2 internal + 1 root = 20 nodes
        assert_eq!(calculate_total_nodes(257, 16), 20);
    }

    #[test]
    fn test_div_ceil() {
        assert_eq!(div_ceil(10, 3), 4);
        assert_eq!(div_ceil(9, 3), 3);
        assert_eq!(div_ceil(0, 5), 0);
    }

    #[test]
    fn test_alignment() {
        assert!(is_aligned(0, 4));
        assert!(is_aligned(4, 4));
        assert!(is_aligned(8, 4));
        assert!(!is_aligned(1, 4));
        assert!(!is_aligned(6, 4));

        assert_eq!(align_up(0, 4), 0);
        assert_eq!(align_up(1, 4), 4);
        assert_eq!(align_up(3, 4), 4);
        assert_eq!(align_up(4, 4), 4);
        assert_eq!(align_up(5, 4), 8);
    }
}
