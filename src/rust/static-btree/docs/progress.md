# Static B+Tree Development Progress

This file tracks the incremental progress of the `static-btree` crate inside **FlatCityBuf**.

## Legend

- `[x]` = completed
- `[~]` = in‑progress / partly done
- `[ ]` = not started

## Milestones

| # | Milestone | Tasks | Status |
|---|-----------|-------|--------|
| 1 | Core infrastructure | • Define `Key` trait<br>• Implement primitive + custom key types<br>• Implement `Entry` struct | `[x]` Done |
| 2 | Implementation plan | • Draft initial policy<br>• Review feedback & iterate | `[x]` Updated  ✅ (see implementation_plan.md) |
| 3 | Tree search API     | • Design `StaticBTree` struct & public API<br>• Lower‑bound & range search handling duplicates<br>• Streaming node reads | `[~]` `find_exact` has been implemented. `find_range` is not implemented yet. |
| 4 | Payload handling    | • Implement payload handling<br>• Implement duplicate handling | `[ ]` |
| 5 | Async / HTTP query  | • `http_find_exact`, `http_find_range` similar to packed_rtree<br>• Feature‑gated under `http` | `[ ]` |
| 6 | Testing & Benchmarks| • Unit tests for all key types & duplicate cases<br>• Criterion benchmark suite | `[ ]` |

## Recent Activity

- 2025-04-23: `find_exact` has been implemented. `find_range` is not implemented yet.

## Next Steps

1. Implement `find_range` and add tests
2. Add handling payloads and duplicates in the tree.
3. Write unit tests (start with u32 and duplicate scenarios).
4. Prototype `http_stream_query` using packed_rtree's client abstraction.

## Task Guidelines for Contributors & LLMs

### Development Workflow

1. **Sync & Build**

  ```bash
  cargo test -p static-btree | cat   # fast feedback loop
  ```

2. **Focus Area** – pick the *earliest* `[ ]` item in the milestone table unless otherwise coordinated.  Keep pull requests small and focused.
3. **Coding Standards** – follow `rust.mdc` rules (no `unwrap`, prefer channels over mutexes, use `thiserror` for custom errors).  All logs must be lowercase.
4. **Docs First** – update `implementation_plan.md` *before* large refactors/additions so the design remains explicit.

### File Overview

```
static-btree
├── src
│   ├── key.rs          # key trait & impls (✅ done)
│   ├── entry.rs        # key‑offset pair (✅ done)
│   ├── stree.rs         # StaticBTree search logic (��️ milestone 3)
│   └── error.rs        # crate::error::Error (✅ done)
└── docs
    ├── implementation_plan.md
    └── progress.md
```

### PR Checklist

1. `cargo test` – all green.
2. `cargo fmt` – no diff.
3. Update `progress.md` status lines.
4. Explain *why* in the PR description; include performance numbers if relevant.

Happy hacking 👩‍💻👨‍💻
