## What is the requirements for attributes indexing?

### ⭐Important

- Exact match
    - Single

        ```sql
        SELECT * FROM city_features WHERE city_name = 'Delft'
        ```

    - Exact match multiple condition

        ```sql
        SELECT * FROM city_features WHERE city_name = 'Delft' AND land_use = 'building'
        ```

- Range queries
    - Numeric

        ```sql
        SELECT * FROM city_features WHERE height > 20;
        ```

    - Date/Time range

        ```sql
        SELECT * FROM city_features WHERE construction_date BETWEEN '1950-01-01' AND '2000-12-31';
        ```

    - String Prefix/Suffix (❌ less important)

        ```sql
        SELECT * FROM city_features WHERE owner LIKE 'John%';
        ```


### ❌Not important

Bc this can be done with app layer

- Aggregation
    - Count

        ```sql
        SELECT COUNT(*) FROM city_features WHERE height > 100;
        ```

    - Sum/Ave

        ```sql
        SELECT SUM(*) FROM city_features GROUP BY city;
        ```


## Requirements for query strategy

### Primary

- Fast query for
    - exact match
    - range query
- Works well for file system (not just in-memory). But as long as the index structure isn’t huge, we can also assume it can load whole into the memory
- Should perform well in average case and worse case

### Less important

- Atomicity for data access (since it’s not database server that multiple client simultaneously access)
- Memory efficiency and time complexity when insertion and deletion

## What strategies we can use for these queries

### Hash-based indexing

Use hash table or hashmap to store attributes and corresponding feature offset.

**Pros**

- simple to implement
- fast query for exact match
- Fast insertion and deletion

**Cons**

- Not ideal for other queries like range, prefix, etc

**Time Complexity**

- Search
    - Average $O(1)$
    - Worst  $O(N)$
- Insert
    - Average $O(1)$
    - Worst  $O(N)$
- Space complexity
    - Average $O(N)$
    - Worse $O(N)$

### B-tree/B+Tree

**Pros**

- Good for range queries
- Balances well for equality queries and range queries

**Cons**

- Slower than hash-based for exact match

**Time complexity**

- Insertion/deletion
    - Average $O(log N)$
    - Worst $O(log N)$
- Search
    - Average $O(log N)$
    - Worst  $O(log N)$
- Space complexity
    - Average $O(N)$
    - Worse $O(N)$

### Sorted Array with Binary Search

**Pros**

- Best for exact match queries and fast range scans
- Minimal memory overhead
- Cache-efficient

**Cons**

- Insertion/deletion requires full reordering

**Time Complexity**

- Insertion/deletion
    - Average $O(logN)$
    - Worst $O(N)$
- Search
    - If balanced $O(log N)$
    - if unbalanced $O(N)$
- Space complexity
    - Average $O(N)$
    - Worse $O(N)$

### Segment tree

https://cp-algorithms.com/data_structures/segment_tree.html

**Pros**

- Best for range queries as tree itself accommodates range
- Supports exact match queries

**Cons**

- Higher memory usage than BST/B-tree
- Not efficient for disk storage (better for in-memory queries)

### Bitmap Index

[https://en.wikipedia.org/wiki/Bitmap_index#:~:text=A bitmap index is a,records that contain the data](https://en.wikipedia.org/wiki/Bitmap_index#:~:text=A%20bitmap%20index%20is%20a,records%20that%20contain%20the%20data).

**Pros**

- Best for low cardinality data e.g. `country`
- Very fast logical combination of multiple constrains

**Cons**

- Not good for high-cardinality data
- Not good for range query

### Trie / Prefix Tree

**Pros**

- Best for string prefix search (not the main case of us)

**Cons**

- Not good for numeric data or range queries (main use-case of ours)

## Conclusion about query strategy

I believe Sorted array with Binary Search will be good for our case because

- We can construct balanced BST as long as we handle static dataset and construct tree from bottom. Unless we care about insertion and deletion, it should be fine.
- In terms of time complexity, it’ll perform the best rather than other search trees like b-tree, etc
- It’ll perform the best in both exact match and range queries