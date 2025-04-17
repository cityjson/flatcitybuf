# Static B+Tree (S+Tree) Implementation Plan

**Project:** Implement the `static-btree` Rust Crate

**Goal:** Create a Rust crate for a Static B+Tree (S+Tree) optimized for read performance.

## 1. Introduction

This document outlines the implementation strategy and detailed Rust API signatures for a static, implicit B+Tree (S+Tree). The goal is to create a highly performant, read-optimized B+Tree suitable for large, static datasets, emphasizing cache efficiency and minimal memory usage during queries.

The implementation follows the principles described in the [Algorithmica S+Tree article](https://en.algorithmica.org/hpc/data-structures/s-tree/), utilizing an implicit Eytzinger layout for node addressing and storing the entire tree structure contiguously.
YOU SHOULD READ THIS ARTICLE!

## 2. Core Concepts

* **Static:** Built once, read many times. No modifications after build.
* **Implicit Layout (Eytzinger):** Nodes located arithmetically, not via pointers. Stored contiguously, often level-by-level.
* **Packed Nodes:** Nodes are fully utilized (except potentially the last one per level) for better space and cache efficiency.
* **Read Optimization:** Designed for fast lookups and range scans by minimizing I/O (reading only needed nodes).
* **`Read + Seek` Abstraction:** Operates on standard Rust I/O traits, enabling use with files, memory, etc.
