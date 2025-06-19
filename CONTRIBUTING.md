# Contributing to FlatCityBuf 🤝

<div align="center">

**Thank you for your interest in contributing to FlatCityBuf!**

_We welcome contributions from the community and are excited to work with you._

[![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=flat&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

</div>

---

## 🌟 Ways to Contribute

There are many ways to contribute to FlatCityBuf:

- 🐛 **Report bugs** and issues
- 💡 **Suggest new features** or improvements
- 📝 **Improve documentation** and examples
- 🔧 **Submit code changes** and fixes
- 🧪 **Add tests** and benchmarks
- 🌐 **Help with translations** and internationalization
- 💬 **Answer questions** in discussions and issues

---

## 🚀 Getting Started

### Prerequisites

Before contributing, ensure you have:

- **Rust toolchain** (1.83.0 or later)
- **Git** for version control
- **wasm-pack** (for WebAssembly development)
- **cargo-watch** (for development workflow)
- **cargo-audit** (for security checks)

### Development Setup

1. **Fork and Clone**

   ```bash
   # Fork the repository on GitHub, then clone your fork
   git clone https://github.com/YOUR_USERNAME/flatcitybuf.git
   cd flatcitybuf

   # Add upstream remote
   git remote add upstream https://github.com/HideBa/flatcitybuf.git
   ```

2. **Install Development Tools**

   ```bash
   # Install development dependencies
   cargo install cargo-watch cargo-audit cargo-nextest

   # Install pre-commit hooks (optional but recommended)
   pip install pre-commit
   pre-commit install
   ```

3. **Build and Test**

   ```bash
   # Build all crates
   cargo build --workspace --all-features --exclude fcb_wasm --release

   # Run tests
   cargo test --workspace

   # Run with file watching for development
   cargo watch -x test
   ```

4. **Verify Everything Works**

   ```bash
   # Run benchmarks
   cargo bench -p fcb_core --bench read -- --release

   # Check for security vulnerabilities
   cargo audit

   # Run clippy for linting
   cargo clippy --workspace --all-features
   ```

---

## 🔄 Development Workflow

### Branching Strategy

- **`main`** - Production-ready code
- **`develop`** - Integration branch for new features
- **`feature/xyz`** - Feature development branches
- **`bugfix/xyz`** - Bug fix branches
- **`docs/xyz`** - Documentation updates

### Making Changes

1. **Create a Branch**

   ```bash
   # Always branch from the latest main
   git checkout main
   git pull upstream main
   git checkout -b feature/your-feature-name
   ```

2. **Make Your Changes**

   - Follow our [coding standards](#coding-standards)
   - Write tests for new functionality
   - Update documentation as needed
   - Ensure code compiles and tests pass

3. **Test Your Changes**

   ```bash
   # Run the full test suite
   cargo test --workspace

   # Run specific crate tests
   cargo test -p fcb_core

   # Run integration tests
   cargo test --test integration_tests

   # Format code
   cargo fmt --all

   # Check with clippy
   cargo clippy --workspace --all-features -- -D warnings
   ```

4. **Commit Your Changes**

   ```bash
   # Use conventional commit messages
   git add .
   git commit -m "feat: add spatial query optimization"
   ```

5. **Push and Create PR**

   ```bash
   git push origin feature/your-feature-name
   # Then create a Pull Request on GitHub
   ```

---

## 📋 Coding Standards

### Rust Guidelines

We follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/) and these additional standards:

#### **Code Style**

- Use `rustfmt` with default settings
- Follow Rust naming conventions:
  - `snake_case` for variables, functions, modules
  - `PascalCase` for structs, enums, traits
  - `SCREAMING_SNAKE_CASE` for constants
- Prefer explicit, descriptive names over short abbreviations

#### **Error Handling**

- Use `thiserror` for custom errors in libraries
- Avoid `unwrap()` except in tests and examples
- Return `Result<T, E>` for fallible operations
- Use `anyhow` for binary crates when appropriate

#### **Performance**

- Use iterators instead of loops where possible
- Minimize memory allocations
- Prefer borrowed references (`&str`, `&[u8]`) over owned data
- Use `criterion` for benchmarking

#### **Documentation**

- Write rustdoc comments for all public APIs
- Include examples in documentation
- Update docs when changing public interfaces

#### **Testing**

- Write unit tests with `#[cfg(test)]`
- Use integration tests for public APIs
- Add benchmarks for performance-critical code
- Mock external dependencies

### Example Code Structure

````rust
use anyhow::{Context, Result};
use thiserror::Error;

/// Custom error type for the module
#[derive(Error, Debug)]
pub enum MyError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("io error")]
    Io(#[from] std::io::Error),
}

/// Process city data with proper error handling
///
/// # Example
///
/// ```rust
/// use fcb_core::process_data;
///
/// let result = process_data("input.fcb")?;
/// println!("processed {} features", result.len());
/// ```
pub fn process_data(path: &str) -> Result<Vec<Feature>, MyError> {
    let data = std::fs::read(path)
        .context("failed to read input file")?;

    parse_features(&data)
        .context("failed to parse features")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_data() {
        let result = process_data("test_data.fcb").unwrap();
        assert_eq!(result.len(), 10);
    }
}
````

---

## 🧪 Testing Guidelines

### Test Categories

1. **Unit Tests**

   - Test individual functions and methods
   - Use `#[cfg(test)]` modules
   - Mock external dependencies

2. **Integration Tests**

   - Test public APIs and crate interactions
   - Place in `tests/` directory
   - Use real data files when possible

3. **Benchmarks**
   - Use `criterion` for performance tests
   - Place in `benches/` directory
   - Compare against baseline performance

### Writing Good Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        // Arrange
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("test.fcb");
        let original_data = create_test_features();

        // Act
        serialize_to_file(&original_data, &file_path).unwrap();
        let deserialized_data = deserialize_from_file(&file_path).unwrap();

        // Assert
        assert_eq!(original_data, deserialized_data);
    }

    #[test]
    fn test_error_handling_invalid_input() {
        let result = process_data("nonexistent.fcb");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("failed to read"));
    }
}
```

### Running Tests

```bash
# Run all tests
cargo test --workspace

# Run tests with output
cargo test --workspace -- --nocapture

# Run specific test
cargo test test_serialize_deserialize_roundtrip

# Run tests in release mode (for performance testing)
cargo test --release

# Run benchmarks
cargo bench -p fcb_core
```

---

## 📝 Documentation Guidelines

### Code Documentation

- **Public APIs**: Must have rustdoc comments with examples
- **Modules**: Include module-level documentation
- **Examples**: Provide working code examples
- **Error Cases**: Document when functions can fail

### Project Documentation

- Keep README.md up to date
- Update CHANGELOG.md for releases
- Add examples for new features
- Update API documentation links

### Writing Style

- Use clear, concise language
- Include code examples
- Explain the "why" not just the "what"
- Use lowercase for UI/logging text (per project style)

---

## 🐛 Reporting Issues

### Before Reporting

1. **Search existing issues** to avoid duplicates
2. **Check the documentation** and examples
3. **Try the latest version** from main branch

### Issue Template

When reporting bugs, please include:

```markdown
## Bug Description

Brief description of the issue

## Steps to Reproduce

1. Step one
2. Step two
3. Step three

## Expected Behavior

What should have happened

## Actual Behavior

What actually happened

## Environment

- OS: [e.g., macOS 14.0, Ubuntu 22.04]
- Rust version: [e.g., 1.83.0]
- FlatCityBuf version: [e.g., 0.1.0]
- Other relevant info

## Additional Context

Any other relevant information, logs, or screenshots
```

---

## 💡 Feature Requests

We welcome feature requests! Please:

1. **Check existing issues** for similar requests
2. **Describe the use case** and motivation
3. **Propose a solution** if you have ideas
4. **Consider the scope** - start small and iterate

### Feature Request Template

```markdown
## Feature Description

Clear description of the proposed feature

## Use Case

Why is this feature needed? What problem does it solve?

## Proposed Solution

How should this feature work?

## Alternatives Considered

What other approaches did you consider?

## Additional Context

Any other relevant information
```

---

## 🔍 Code Review Process

### For Contributors

- **Keep PRs focused** - one feature or fix per PR
- **Write clear descriptions** explaining what and why
- **Include tests** for new functionality
- **Update documentation** as needed
- **Respond to feedback** promptly and constructively

### Review Criteria

We review for:

- ✅ **Correctness** - Does the code work as intended?
- ✅ **Quality** - Is the code well-written and maintainable?
- ✅ **Performance** - Are there any performance regressions?
- ✅ **Security** - Are there any security concerns?
- ✅ **Documentation** - Is the code properly documented?
- ✅ **Tests** - Are there adequate tests?

### Pull Request Template

```markdown
## Description

Brief description of changes

## Type of Change

- [ ] Bug fix
- [ ] New feature
- [ ] Breaking change
- [ ] Documentation update

## Testing

- [ ] Unit tests pass
- [ ] Integration tests pass
- [ ] Benchmarks run successfully
- [ ] Manual testing performed

## Checklist

- [ ] Code follows style guidelines
- [ ] Self-review completed
- [ ] Documentation updated
- [ ] Tests added/updated
```

---

## 🚀 Release Process

### Versioning

We follow [Semantic Versioning](https://semver.org/):

- **MAJOR** version for incompatible API changes
- **MINOR** version for backwards-compatible functionality
- **PATCH** version for backwards-compatible bug fixes

### Release Checklist

1. Update version numbers in `Cargo.toml` files
2. Update `CHANGELOG.md` with release notes
3. Run full test suite and benchmarks
4. Create release branch and PR
5. Tag release after merge
6. Publish to crates.io

---

## 🌍 Community Guidelines

### Code of Conduct

We are committed to providing a welcoming and inclusive environment. Please:

- **Be respectful** and considerate
- **Be constructive** in discussions and feedback
- **Help others** learn and grow
- **Follow the golden rule** - treat others as you'd like to be treated

### Communication Channels

- **GitHub Issues** - Bug reports and feature requests
- **GitHub Discussions** - General questions and community chat
- **Pull Requests** - Code contributions and reviews

### Getting Help

If you need help:

1. Check the [documentation](https://docs.rs/fcb_core)
2. Search [existing issues](https://github.com/HideBa/flatcitybuf/issues)
3. Ask in [GitHub Discussions](https://github.com/HideBa/flatcitybuf/discussions)
4. Create a new issue if needed

---

## 🎯 Contribution Focus Areas

We're particularly interested in contributions for:

### High Priority

- 🚀 **Performance optimizations** for reading
- 🔍 **Query performance** improvements
- 🧪 **Test coverage** expansion
- 📚 **Documentation** and examples

### Medium Priority

- 🌐 **WebAssembly** functionality and bindings
- 🔧 **CLI tool** enhancements
- 🗺️ **Spatial indexing** optimizations
- 📊 **Benchmarking** infrastructure

### Future Goals

- 🐍 **Python bindings** development
- 📱 **Mobile platform** support
- ☁️ **Cloud integration** features
- 🔌 **Plugin system** architecture

---

## 🏆 Recognition

Contributors will be:

- ✨ **Listed in CONTRIBUTORS.md**
- 🎉 **Mentioned in release notes**
- 🏅 **Featured in project updates**
- 💝 **Eligible for special recognition**

---

## 📄 License

By contributing to FlatCityBuf, you agree that your contributions will be licensed under the same [MIT License](LICENSE) that covers the project.

---

<div align="center">

**Thank you for contributing to FlatCityBuf! 🙏**

_Together, we're building the future of 3D city model processing._

**[🏠 Back to README](README.md)** • **[📚 Documentation](https://docs.rs/fcb_core)** • **[💬 Discussions](https://github.com/HideBa/flatcitybuf/discussions)**

</div>
