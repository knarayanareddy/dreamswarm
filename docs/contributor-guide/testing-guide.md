# Testing Guide 🧪
Every PR must include tests that cover both success and error cases.

## Test Organization
- **Unit Tests**: Place in the same file as the source code using `#[cfg(test)] mod tests { ... }`. Best for logic, edge cases, and stateless functions.
- **Integration Tests**: Place in the `tests/` directory. Best for multi-module interactions, swarm orchestration, and daemon lifecycles.

## Patterns
### Succeed/Fail
Always test the happy path and the error path.
```rust
#[tokio::test]
async fn test_success() { ... }

#[tokio::test]
async fn test_failure_on_invalid_input() { ... }
```

### Temporary Directories
Use `tempfile::TempDir` for any test that writes to the filesystem. Avoid hardcoded paths like `/tmp/dreamswarm`.
```rust
let tmp = tempfile::TempDir::new().unwrap();
let memory = MemorySystem::new(tmp.path().to_path_buf()).unwrap();
// Test logic...
```

### Async Tests
Use `#[tokio::test]` for any test involving `async/await`.

## Coverage
Aim for ≥70% coverage on new code. Use `make coverage` to generate an HTML report.
