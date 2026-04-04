## Description
<!-- What does this PR do? Explain the motivation and approach. -->

## Type of Change
<!-- Mark the relevant option with an "x" -->
- [ ] Bug fix (non-breaking change that fixes an issue)
- [ ] New feature (non-breaking change that adds functionality)
- [ ] Breaking change (fix or feature that would cause existing functionality to change)
- [ ] Documentation update
- [ ] Refactoring (no functional change)
- [ ] Test improvement
- [ ] CI/CD change

## Module(s) Affected
<!-- Which module(s) does this change touch? -->
- [ ] `runtime/` (agent loop, session, permissions, config)
- [ ] `tools/` (tool implementations)
- [ ] `memory/` (3-layer memory system)
- [ ] `context/` (compression engine)
- [ ] `swarm/` (multi-agent orchestration)
- [ ] `daemon/` (KAIROS)
- [ ] `dream/` (autoDream)
- [ ] `query/` (LLM interface)

## Checklist
- [ ] Code compiles without warnings
- [ ] All tests pass (`make test`)
- [ ] Code is formatted (`cargo fmt`)
- [ ] Clippy is happy (`cargo clippy -- -D warnings`)
- [ ] New code has tests
- [ ] Doc comments on all public items
- [ ] Commit messages follow conventional commit format

## Security
<!-- Does this PR touch a trust boundary? -->
- [ ] This PR does NOT touch any security-sensitive code
- [ ] This PR touches security-sensitive code and needs `security-review` label

## Testing
<!-- How was this tested? -->
```bash
# Commands used to test
cargo test my_test_name
```
