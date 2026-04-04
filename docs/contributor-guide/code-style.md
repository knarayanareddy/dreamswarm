# Code Style Guide ✍️
DreamSwarm follows standard Rust conventions with a few project-specific additions.

## Formatting & Lints
1. **`cargo fmt`**: Required for all commits. We use the default nightly style.
2. **`cargo clippy`**: We use `--pedantic`. All warnings must be resolved before merging.
3. **`make check`**: Fast check for compilation and lints.

## Naming Conventions
- **`snake_case`**: Variables, functions, modules.
- **`PascalCase`**: Structs, Enums, Traits.
- **`SCREAMING_SNAKE_CASE`**: Constants and statics.

## Documentation
- **Public Items**: All `pub` items must have `///` doc comments.
- **Explain "Why"**: Comments should focus on intent and edge cases, not what the code does.

## Error Handling
- **`anyhow`**: Use for application-level error propagation.
- **`thiserror`**: Use for defining custom library-level error types in core modules.
- **No `unwrap()`**: Never use `unwrap()` in non-test code. Use `context()` or explicit error handling.

## Async
- **Tokio**: Use the Tokio runtime for all async operations.
- **Trait Sync**: All tools must be `Send + Sync`.
