# FlatCityBuf workspace justfile.
#
# Every language directory has its own justfile exposing the SAME interface:
#
#     just check    # lint + type + test + build, read-only
#     just test | lint | type | build
#     just fix      # the only recipe that rewrites source
#
# The recipes here fan each one out across all five, in dependency order.
# To work on one language, cd into it and use the same verbs:
#
#     cd src/py && just test
#     cd src/cpp && just check
#
# Directory order matters: examples/web consumes src/ts/dist through a `file:`
# dependency, so src/ts must be built before examples/web is touched.

DIRS := "src/rust src/cpp src/py src/ts examples/web"

# List all available commands
default:
    @just --list

# ============================================================================
# Unified workspace verbs
# ============================================================================

# Verify EVERYTHING, read-only: lint, type check, tests and build, for all four
# reader implementations plus the web example. Never rewrites a file — that is
# what `just fix` is for.

# Lint + type + test + build, every language, read-only
check: (_each "check")

# Tests only, every language
test: (_each "test")

# src/ts and examples/web have no linter configured; they say so and pass.

# Linters and format checks only, every language
lint: (_each "lint")

# Rust: cargo check. C++: the compiler. Python: mypy --strict. TS: tsc --noEmit.

# Type checks only, every language
type: (_each "type")

# Builds only, every language
build: (_each "build")

# Apply every automatic fix (rustfmt, clippy --fix, ruff, clang-format) — MUTATES
fix: (_each "fix")

# Run one recipe in every language justfile, in DIRS order, first failure wins
_each recipe:
    #!/usr/bin/env bash
    set -euo pipefail
    for d in {{DIRS}}; do
      printf '\n\033[1m==> %s: just %s\033[0m\n' "$d" "{{recipe}}"
      (cd "$d" && just {{recipe}})
    done

# ============================================================================
# Workspace-level tasks (not language-specific)
# ============================================================================

# conformance/*.fcb and *.expected.jsonl are tracked, and regeneration is NOT
# byte-reproducible (cjseq2 iterates CityObjects from a HashMap with per-process
# ordering) -- so this always dirties the working tree, even when nothing
# semantic changed. Diff the *parsed* JSON, not the raw bytes, before committing;
# don't commit pure churn.

# Regenerate the conformance corpus (needs the Rust CLI)
gen-conformance:
    ./scripts/gen_conformance.sh

# Regenerate every committed FlatBuffers binding (C++, Python, TypeScript)
gen-all:
    cd src/cpp && just gen-fbs
    cd src/py && just gen-fbs
    cd src/ts && just gen-fbs

# Start dev container
devcon:
    devcontainer up --workspace-folder .
    devcontainer exec --workspace-folder . bash

# Rebuild dev container from scratch
devcon-build:
    devcontainer build --workspace-folder . --no-cache
    just devcon

# Install development tools
install-tools:
    cargo install just cargo-nextest cargo-watch cargo-audit

# Remove every build artifact, in every language
clean:
    cd src/rust && just clean
    cd src/cpp && just clean
    cd src/ts && just clean
    cd examples/web && just clean

# Generate API documentation for every language
docs: (_each "docs")
