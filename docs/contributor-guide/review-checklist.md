# PR Review Checklist 📝
Standard review criteria for all Pull Requests.

## Review Criteria
- **Correctness**: Does it do what it claims?
- **Safety**: Could this introduce a security issue? (Especially in `tools/`, `daemon/`, `permissions/`)
- **Tests**: Is the new code tested? Are edge cases covered?
- **Performance**: Does it add unnecessary allocations or API calls?
- **Architecture**: Does it fit the existing patterns? (See `ARCHITECTURE.md`)
- **Documentation**: Are public APIs documented?
- **Simplicity**: Is this the simplest correct solution?

## Module-Specific Checks
- **Core Loop**: Keep the loop simple. Avoid side effects.
- **Permissions**: Every tool must have a `RiskLevel`.
- **Memory**: Every memory operation must be bandwidth-aware.
- **Context**: Tokens are a scarce resource. Use them wisely.

## Review Process
1. **Triage**: A maintainer will label your PR within 24 hours.
2. **Review**: A maintainer will review within 48 hours.
3. **Feedback**: If changes are needed, you'll get specific, actionable feedback.
4. **Approval**: Once approved, a maintainer will merge.
