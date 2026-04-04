# Commit Conventions 🛠️
We use Conventional Commits to automate changelog generation and semantic versioning.

## Message Format
```text
<type>(<scope>): <description>
[optional body]
[optional footer(s)]
```

## Standard Types
- **`feat`**: New feature (mapped to minor version).
- **`fix`**: Bug fix (mapped to patch version).
- **`docs`**: Documentation only.
- **`refactor`**: Code change that neither fixes a bug nor adds a feature.
- **`test`**: Adding or correcting tests.
- **`perf`**: Performance improvement.
- **`ci`**: CI/CD changes.
- **`chore`**: Maintenance (dependencies, build scripts).

## Scopes
Use modules names like `runtime`, `tools`, `memory`, `context`, `swarm`, `daemon`, `dream`, `query`.

## Examples
- `feat(tools): add GitDiffTool for viewing uncommitted changes`
- `fix(daemon): prevent heartbeat from drifting under heavy load`
- `docs(memory): clarify 3-layer architecture in contributor guide`

## Breaking Changes
Indicate with a `!` after the type/scope and includes `BREAKING CHANGE:` in the footer.
```text
feat(runtime)!: change Tool trait to require async validate method
```
