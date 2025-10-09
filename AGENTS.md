# AGENTS.md

Agent guidance for working with the FlatCityBuf codebase. This file is shared across multiple AI coding assistants (Claude Code, Cursor, Cline, etc.).

## Project Overview

FlatCityBuf is a cloud-optimized binary format for storing and retrieving 3D city models. It combines the semantic richness of CityJSON with the performance benefits of FlatBuffers binary serialization and advanced spatial indexing techniques. The project enables efficient partial data retrieval from cloud storage using HTTP range requests.

### Key Features

- **Zero-Copy Access**: FlatBuffers enables direct memory access without parsing
- **HTTP Range Optimization**: File structure aligned for efficient partial downloads
- **Hilbert Ordering**: Features sorted by Hilbert curve for spatial locality
- **Hierarchical Geometry**: Efficient encoding of 3D boundaries using dimensional arrays
- **Extension Support**: Full compatibility with CityJSON extension mechanism

### Design Goals

1. **Performance**: Reduce processing overhead using FlatBuffers' zero-copy access and optimize storage via binary encoding
2. **Cloud Compatibility**: Enable partial data retrieval via HTTP Range Requests with spatial sorting and indexing
3. **Scalability**: Ensure interoperability with existing GIS tools (QGIS, Cesium, Mapbox) and reduce cloud storage costs
4. **User Experience**: Faster downloads of arbitrary 3D city model subsets and instant web application loading

---

## Working with AI Assistants

### General Guidelines for AI Agents

- The user is proficient in programming and requests AI assistance to save time, not for basic explanations
- If a test fails more than twice in a row, analyze the situation and collaborate with the user rather than trial-and-error testing
- Write code with explanations and use test cases to verify correctness
- If context is unclear, confirm with the user before proceeding
- When asked to "memory it", update relevant documentation files in `.cursor/rules/memory/` (productContext.md, specification.md, etc.)

### Memory Bank System

AI agents operate with memory resets between sessions. The memory bank under `.cursor/rules/memory/` is mandatory reading at the start of every task:

**Core Files:**
- `productContext.md` - Project purpose, problem statement, and user experience goals
- `specification.md` - Detailed FlatCityBuf specification, encoding strategy, and design decisions

**Additional Context:**
- Store complex feature documentation, integration specs, API docs, testing strategies, and deployment procedures under `.cursor/rules/memory/`

---

## Architecture Overview

### Core Components

1. **FlatBuffers Schemas** (`/src/fbs/`)
   - `header.fbs` - File metadata, transformations, and index information
   - `geometry.fbs` - 3D geometry structures and semantic surfaces
   - `feature.fbs` - City objects and their attributes
   - `extension.fbs` - Support for CityJSON extensions

2. **Rust Workspace** (`/src/rust/`)
   - `fcb_core` - Core library for reading/writing FlatCityBuf format
   - `cli` - Command-line interface for file conversion and analysis
   - `fcb_wasm` - WebAssembly bindings for browser usage
   - `fcb_py` - Python bindings using PyO3 and maturin
   - `fcb_api` - HTTP API server for FlatCityBuf operations

3. **Indexing Structures**
   - **Packed R-tree**: 2D spatial indexing using Hilbert space-filling curves for bbox/point/nearest-neighbor queries
   - **Static B+Tree**: Attribute-based indexing with efficient range queries and exact match queries
   - Both indices store byte offsets for direct feature access

### File Format Structure

```
┌─────────────────┐
│  Magic Bytes    │  8 bytes - File identifier (fcb\0\1\0\0\0\0\0)
├─────────────────┤
│  Header Size    │  4 bytes - Size of header
├─────────────────┤
│     Header      │  Variable - Metadata & schema
├─────────────────┤
│   R-tree Index  │  Variable - Spatial index
├─────────────────┤
│ Attribute Index │  Variable - B+Tree indices
├─────────────────┤
│    Features     │  Variable - City objects
└─────────────────┘
```

### Query Execution Flow

**Spatial Queries:**
- Traverse R-tree using HTTP range requests
- Retrieve feature offsets from leaf nodes
- Batch nearby features to minimize requests

**Attribute Queries:**
- Traverse B+Tree index to find matching keys
- Handle duplicates via payload section with prefetching and batch resolution
- Use prefetching and batch resolution for efficiency

**Feature Retrieval:**
- Use offsets to fetch specific byte ranges
- Decode features using FlatBuffers zero-copy access
- Process geometry and attributes on demand

---

## Common Development Commands

### Python Development

```bash
# Build and install Python bindings in development mode
cd src/rust && make py-develop

# Build Python wheel for distribution
cd src/rust && make py-build

# Run Python tests
cd src/rust && make py-test

# Clean Python build artifacts
cd src/rust && make py-clean
```

### Rust Building and Testing

```bash
# Build all Rust crates (excluding WASM)
cd src/rust
cargo build --workspace --all-features --exclude fcb_wasm --release

# Run pre-commit checks (formatting, linting, tests)
cd src/rust && make pre-commit

# Run tests with nextest (faster test runner)
cd src/rust
cargo nextest run --all-features --workspace --exclude fcb_wasm --exclude fcb_py

# Run specific integration test
cd src/rust && cargo test -p fcb_core --test e2e

# Run benchmarks
cd src/rust && cargo bench -p fcb_core --bench read -- --release
```

### Code Generation

```bash
# Generate Rust code from FlatBuffers schemas
make gen-all  # Runs all generation scripts
# OR specifically:
./scripts/gen_rust.sh  # Generates Rust bindings from .fbs files
```

### WebAssembly Build

```bash
# Build WASM module for release
cd src/rust/wasm && wasm-pack build --target web --release --out-dir ../../ts

# Build WASM module for debug
cd src/rust && make wasm-build
```

### Linting and Formatting

```bash
cd src/rust

# Format code
cargo fmt --all

# Run clippy with auto-fix
cargo clippy --fix --allow-dirty --workspace --all-targets --all-features --exclude fcb_wasm

# Run clippy for WASM target
cargo clippy --fix --allow-dirty -p fcb_wasm --target wasm32-unknown-unknown

# Check for security vulnerabilities
cargo audit
```

---

## Language-Specific Guidelines

### Rust Development

**General Principles:**
- Write idiomatic Rust code: clear, efficient, and maintainable
- Prioritize safety, performance, and modularity
- Follow Rust naming conventions: `snake_case` for variables/functions, `PascalCase` for types, `SCREAMING_SNAKE_CASE` for constants
- Keep code DRY using functions, modules, and generics
- Use explicit, descriptive names for variables, functions, and types
- Avoid `unwrap()` except in test cases; ensure proper error handling
- Fix grammar mistakes in comments when found

**Error Handling:**
- Use `thiserror` for custom error types in library code (NOT `anyhow` unless explicitly approved)
- Avoid panics in library code; return errors instead
- Handle errors and edge cases early

**Performance:**
- Use iterators instead of loops for better performance and readability
- Minimize memory allocations by using borrowed references (`&str`, `&[u8]`)
- Optimize for human readability while maintaining machine efficiency
- Use `criterion` for benchmarking

**Async Programming:**
- Use `tokio` as the async runtime
- Prefer channels over mutexes where applicable
- Implement structured concurrency using `tokio::select!`
- Use `tokio::sync::mpsc` for multi-producer, single-consumer communication

**API Design:**
- Follow Rust's API guidelines for public interfaces
- Use builder patterns for complex configurations

**Testing:**
- Write unit tests with `#[cfg(test)]`
- Use integration tests for public APIs in `tests/` directory
- Mock external dependencies where necessary
- Use `tokio::test` for async tests

**Documentation:**
- Write Rustdoc comments for public functions and structs
- Include examples in documentation

**Dependency Management:**
- Use `cargo-audit` to check for vulnerabilities
- Keep dependencies minimal and up-to-date
- Add crates to workspace's `Cargo.toml` file, not individual crates

**Logging:**
- Use `tracing` for structured logging
- Enable debug assertions with `debug_assert!()`

### TypeScript/JavaScript Development

**Development Philosophy:**
- Write clean, maintainable, and scalable code
- Follow SOLID principles
- Prefer functional and declarative programming patterns
- Emphasize type safety and static analysis
- Practice component-driven development

**Code Style:**
- Write concise, modular TypeScript code
- Prefer functional components over class components
- Avoid code duplication; prioritize iteration and modularization
- Use descriptive variable names with auxiliary verbs (e.g., `isLoading`, `hasError`)
- File structure: exported component, subcomponents, helpers, static content, types

**Naming Conventions:**
- Directories & Files: kebab-case (e.g., `components/auth-wizard`)
- Components: PascalCase (e.g., `UserProfile`)
- Variables, Functions, Methods, Props: camelCase (e.g., `fetchData`)
- Boolean Variables: Prefix with verbs (e.g., `isLoading`, `hasError`)
- Event Handlers: Prefix with `handle` (e.g., `handleClick`)
- Custom Hooks: Prefix with `use` (e.g., `useAuth`)
- Constants & Environment Variables: UPPER_CASE (e.g., `API_URL`)

**TypeScript Usage:**
- Use TypeScript for all code
- Prefer interfaces over types
- Avoid enums; use maps instead
- Enable strict mode for better type safety
- Use TypeScript utility types (`Partial`, `Pick`, `Omit`)
- Apply generics where necessary

**Syntax and Formatting:**
- Use single quotes (`'`) for strings
- Use early returns to improve readability
- Use `const` whenever possible
- Omit semicolons unless required for disambiguation
- Always use strict equality (`===`)
- Use Prettier for consistent formatting
- Keep line length under 80 characters

**UI and Styling:**
- Use Tailwind CSS for utility-based styling
- Use Shadcn UI for accessible components
- Use Radix UI primitives where necessary
- Follow mobile-first responsive design
- Implement dark mode using CSS variables or Tailwind
- Ensure high accessibility (a11y) standards using ARIA roles and semantic HTML

**State Management:**
- Use `useState` for component-level state
- Use `useReducer` for complex state logic
- Use `useContext` for shared state
- Use Redux Toolkit for global state management

**Performance:**
- Minimize the use of `useState` and `useEffect`
- Use `useCallback` to memoize functions
- Use `useMemo` for expensive calculations
- Implement code splitting using dynamic imports

**Error Handling:**
- Use Zod for schema validation
- Implement error boundaries for UI stability
- Handle errors at the start of functions
- Use early returns instead of deeply nested `if` statements

**Testing:**
- Use Jest and React Testing Library for unit tests
- Follow Arrange-Act-Assert pattern
- Mock external dependencies
- Ensure full keyboard navigation support in accessibility tests

**Security:**
- Sanitize user input to prevent XSS attacks
- Use DOMPurify for HTML sanitization
- Implement secure authentication methods
- Ensure API communication is encrypted over HTTPS

**Documentation:**
- Use JSDoc for documenting functions and interfaces
- Document public APIs and components
- Include examples where applicable

---

## Git Workflow

### Commit Best Practices

1. **Review Changes**
   ```bash
   git status
   git diff
   git log
   ```

2. **Analyze Changes**
   - Identify modified or added files
   - Understand the nature of changes (new feature, bug fix, refactoring, etc.)
   - Evaluate the impact on the project
   - Ensure no sensitive information is exposed

3. **Create Meaningful Commit Messages**
   ```bash
   git commit -m "fix: Resolve issue with authentication timeout"
   ```

### Pull Request Best Practices

1. **Review Branch Status**
   ```bash
   git status
   git diff main...HEAD
   git log
   ```

2. **Analyze Changes**
   - Review all commits made since branching off `main`
   - Assess change scope and impact
   - Ensure no sensitive data is committed

3. **Create a Pull Request**
   ```bash
   gh pr create --title "feat: Improve Rust error handling" --body "Improved error handling with Result<T, E>."
   ```

---

## Important Notes

- The project uses a Rust workspace structure - always build from `/src/rust/`
- WASM builds require `wasm-pack` and target `wasm32-unknown-unknown`
- Python bindings require `uv` package manager and use maturin for building
- FlatBuffers schemas must be regenerated after changes using `make gen-all`
- HTTP optimization is critical - always consider range request efficiency
- Geometry templates are supported for efficient repeated geometry encoding
- Use `thiserror` for custom error types in library code, avoid `anyhow` except when explicitly approved

---

## Development Tools

### Local MCP Tool: `serena`

`serena` is a tool that can be used to get an overview of the code base.

---

## Testing Strategy

- **Unit tests**: Use `#[cfg(test)]` modules for Rust
- **Integration tests**: Place in `/tests/` directories
- **Use cargo nextest**: For faster test execution
- **Mock HTTP clients**: For remote data tests
- **Python tests**: Use `uv` and pytest
- **Web tests**: Use Jest and React Testing Library

---

## Performance Considerations

- Minimize HTTP requests through batching
- Use buffered readers with caching
- Prefer iterators over collecting into vectors
- Profile with `criterion` benchmarks
- Implement payload prefetching and batch resolution for attribute queries
- Consider spatial locality when ordering features

---

## Additional Resources

For detailed specification information, see:
- [specification.md](.cursor/rules/memory/specification.md) - Complete FlatCityBuf format specification
- [productContext.md](.cursor/rules/memory/productContext.md) - Project context and goals
