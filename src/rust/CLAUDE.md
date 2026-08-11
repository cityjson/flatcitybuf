# Rust Coding Guidelines for Library Development

The Rust workspace is `fcb_core` (the library), `cli` (`fcb`) and `fcb_api`
(axum server). `fcb_core` is **the authoritative oracle** for the format —
the C++, Python and TypeScript readers are validated against its output, so a
behaviour change here ripples into three other implementations and the
conformance corpus. See the root `CLAUDE.md`.

## General Principles

- Write **idiomatic Rust** code that is clear, efficient, and maintainable.
- Prioritize **safety, performance, and modularity**.
- Follow **Rust’s naming conventions**:
  - Use `snake_case` for variables, functions, and module names.
  - Use `PascalCase` for structs, enums, and traits.
  - Use `SCREAMING_SNAKE_CASE` for constants and static variables.
- Keep code **DRY (Don't Repeat Yourself)** by using functions, modules, and generics.
- Use **explicit, descriptive names** for variables, functions, and types.
- **Avoid `unwrap()` except in test cases**, ensuring proper error handling.
- **Use generics, traits, and interface programming** where applicable.
- **If any grammar mistakes are found in comments, suggestions for improvement should be provided.**

---

## Error Handling

- Use `thiserror` to make custom error for package-level errors. You shouldn't use `anyhow` unless I explictly approve you to do that.
- Avoid panics in library code; return errors instead.
- Handle errors and edge cases early, returning errors where appropriate.

---

## Performance Optimization

- Use **iterators instead of loops** for better performance and readability.
- Minimize memory allocations by using **borrowed references (`&str`, `&[u8]`)** where possible.
- Optimize for **human readability** while maintaining machine efficiency.
- Use `criterion` for benchmarking.

---

## Async Programming

- Use `tokio` as the async runtime.
- Prefer **channels over mutexes** where applicable.
- Implement **structured concurrency** using `tokio::select!`.
- Use `tokio::sync::mpsc` for multi-producer, single-consumer communication.
- Use `tokio::sync::broadcast` for broadcasting messages.

---

## API Design

- Follow **Rust’s API guidelines** for public interfaces.
- Use **builder patterns** for complex configurations.
- Define and implement traits to invert dependencies and improve testability.
- reexport public types and functions from the root crate.

---

## Testing

- Write **unit tests** with `#[cfg(test)]`.
- Use **integration tests** for public APIs in the `tests/` directory.
- Mock external dependencies where necessary.
- Use `#[tokio::test]` for async tests.
- `cargo nextest run` is the runner. From this directory: `just test` (or
  `just check` for lint + type + test + build). See the root `CLAUDE.md` for
  the workspace-wide verbs.

---

## Documentation

- Write **Rustdoc** comments for public functions and structs.
- Include runnable examples; they are compiled as doctests.

---

## Dependency Management

- Use `cargo-audit` to check for known vulnerabilities. Keep dependencies
  **minimal and up-to-date**.
- Add crates to the workspace `Cargo.toml`. Don't add them to individual
  crates' `Cargo.toml` files — reference them with `{ workspace = true }`.

---

## Logging and Debugging

- Use `tracing` for structured logging.
- Enable debug assertions with `debug_assert!()`.
