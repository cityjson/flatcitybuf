# Static B+Tree Processing Workflows

Below are Mermaid diagrams that illustrate the key workflows for the Static B+Tree implementation: construction, search, and range query operations.

## Tree Construction Workflow

```mermaid
flowchart TD
    A[Start: Sorted Entries] --> B[Calculate Tree Parameters]
    B --> C{Is Number of Entries > 0?}
    C -- No --> D[Create Empty Tree]
    C -- Yes --> E[Calculate Tree Height]
    E --> F[Calculate Node Counts]
    F --> G[Allocate Memory for Nodes]
    G --> H[Create Leaf Nodes]

    subgraph Leaf Node Creation
        H --> H1[Divide Entries into Groups]
        H1 --> H2[For each group, create a leaf node]
        H2 --> H3[Store entry keys in leaf node]
    end

    subgraph Internal Node Creation
        H3 --> I[Start from leaf level]
        I --> J[For each level moving upward]
        J --> K[Group child nodes]
        K --> L[For each group, create parent node]
        L --> M[Select separator keys from children]
        M --> N[Store separator keys in parent]
        N --> O{Is this the root level?}
        O -- No --> J
        O -- Yes --> P[Set Root Node]
    end

    P --> Q[Store Values Array]
    Q --> R[Create Final STree Structure]
    R --> S[End: Tree Constructed]

    D --> S
```

## Tree Structure Visualization

```mermaid
graph TB
    subgraph "Logical B+Tree Structure"
        Root["Root Node (Level 0)"] --> N1["Internal Node (Level 1)"]
        Root --> N2["Internal Node (Level 1)"]
        Root --> N3["Internal Node (Level 1)"]

        N1 --> L1["Leaf Node"]
        N1 --> L2["Leaf Node"]
        N1 --> L3["Leaf Node"]

        N2 --> L4["Leaf Node"]
        N2 --> L5["Leaf Node"]
        N2 --> L6["Leaf Node"]

        N3 --> L7["Leaf Node"]
        N3 --> L8["Leaf Node"]
        N3 --> L9["Leaf Node"]
    end

    subgraph "Physical Memory Layout (Node Index)"
        direction LR
        P0["Node 0\n(Root)"] --> P1["Node 1"] --> P2["Node 2"] --> P3["Node 3"] --> P4["Node 4"] --> P5["Node 5"]
        style P0 fill:#f9f,stroke:#333,stroke-width:2px
        style P1 fill:#bbf,stroke:#333,stroke-width:2px
        style P2 fill:#bbf,stroke:#333,stroke-width:2px
        style P3 fill:#bbf,stroke:#333,stroke-width:2px
        style P4 fill:#dfd,stroke:#333,stroke-width:2px
        style P5 fill:#dfd,stroke:#333,stroke-width:2px
    end

    subgraph "Eytzinger Layout Formula"
        direction TB
        F1["For node with index k and branch i:"]
        F2["Child index = k × (B+1) + i + 1"]
        F3["Where B = branching factor"]

        F1 --> F2 --> F3
        style F1 fill:#ffe,stroke:#333,stroke-width:1px
        style F2 fill:#ffe,stroke:#333,stroke-width:1px
        style F3 fill:#ffe,stroke:#333,stroke-width:1px
    end
```

## Search Operation Workflow

```mermaid
flowchart TD
    A[Start: Search for Key] --> B[Initialize at Root Node]
    B --> C{Is Current Node a Leaf?}

    C -- No --> D[Use Binary Search to Find Branch]
    D --> E[Calculate Child Node Index]
    E --> F[Load Child Node]
    F --> C

    C -- Yes --> G[Search for Key in Leaf Node]
    G --> H{Key Found?}

    H -- No --> I[Return None]
    H -- Yes --> J[Calculate Value Index]
    J --> K[Return Associated Value]

    I --> L[End: Search Completed]
    K --> L
```

## Range Query Workflow

```mermaid
flowchart TD
    A[Start: Range Query] --> B[Search for Start Key]
    B --> C[Find Leaf Node Containing Start Key]
    C --> D[Initialize Empty Result Set]
    D --> E[Scan Current Leaf Node]

    E --> F{Key <= End Key?}
    F -- Yes --> G[Add Key-Value Pair to Results]
    G --> H{More Keys in Node?}
    H -- Yes --> E
    H -- No --> I[Is This the Last Leaf?]

    F -- No --> J[Return Result Set]
    I -- Yes --> J

    I -- No --> K[Calculate Next Leaf Node Index]
    K --> L[Load Next Leaf Node]
    L --> E

    J --> M[End: Range Query Completed]
```

## HTTP Range Request Workflow

```mermaid
flowchart TD
    A[Start: HTTP-Based Query] --> B[Parse Query Requirements]
    B --> C[Determine Required Nodes]

    C --> D[Check Node Cache]
    D --> E{Nodes in Cache?}

    E -- All Found --> F[Return Cached Nodes]
    E -- Some/None Found --> G[Calculate Byte Ranges]

    G --> H[Optimize Byte Ranges]
    H --> I[Issue HTTP Range Requests]

    I --> J[Receive Responses]
    J --> K[Parse Node Data]

    K --> L[Update Node Cache]
    L --> M[Process Query Using Nodes]

    F --> M

    M --> N[Return Query Results]
    N --> O[End: Query Completed]

    subgraph Range Optimization
        H1[Identify Adjacent Ranges] --> H2[Merge Overlapping Ranges]
        H2 --> H3[Batch Small Ranges]
        H3 --> H4[Prioritize Cache Misses]
    end
    H -.-> Range Optimization
```

## Key Comparison During Search

```mermaid
flowchart LR
    A[Target Key]

    subgraph "Internal Node"
        K1[Key 1]
        K2[Key 2]
        K3[Key 3]

        P0[Branch 0]
        P1[Branch 1]
        P2[Branch 2]
        P3[Branch 3]

        P0 --> K1 --> P1 --> K2 --> P2 --> K3 --> P3
    end

    A --> C[Binary Search]
    C --> D{A < K1?}
    D -- Yes --> E[Select Branch 0]
    D -- No --> F{A < K2?}
    F -- Yes --> G[Select Branch 1]
    F -- No --> H{A < K3?}
    H -- Yes --> I[Select Branch 2]
    H -- No --> J[Select Branch 3]
```

These diagrams provide a visual representation of how the Static B+Tree is constructed from sorted entries and how it's used to efficiently perform search and range query operations, including the optimization for HTTP-based data retrieval.

The tree construction builds nodes in a specific memory layout that enables efficient navigation without explicit pointers. The search and range query operations leverage this structure to minimize memory accesses and optimize for cache locality.

For cloud-based applications, the HTTP workflow shows how range requests can be optimized to reduce latency when querying data stored in remote storage.
