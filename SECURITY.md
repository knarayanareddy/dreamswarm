# Security Policy

## 🛡️ Reporting Vulnerabilities
**DO NOT open a public issue for security vulnerabilities.**

Instead, report them via:
1. **GitHub Security Advisories**: [Report a vulnerability](https://github.com/dreamswarm/dreamswarm/security/advisories/new) (preferred).
2. **Email**: `security@dreamswarm.dev`

### What to Include
- Description of the vulnerability.
- Steps to reproduce.
- Potential impact.
- Suggested fix (if you have one).

### What to Expect
- **Acknowledgment**: Within 24 hours.
- **Assessment**: Within 72 hours.
- **Fix Timeline**: Critical — 7 days. High — 14 days. Medium — 30 days.
- **Credit**: You will be credited in the advisory (unless you prefer anonymity).

---

## 🏗️ Security Architecture
DreamSwarm has four trust boundaries. Changes to any of these require extra security review:

### 1. Permission System (`runtime/permissions.rs`)
The 5-layer permission model gates all tool execution. Changes here can expose users to unsafe operations.
**High-risk areas**: Bash command validators, permission escalation logic, and allow/deny pattern matching.

### 2. Mailbox Pattern (`swarm/mailbox.rs`)
Workers communicate with the coordinator through file-based mailboxes. Changes here can allow workers to bypass approval requirements.

### 3. Trust Degradation (`daemon/trust.rs`)
The daemon's trust system controls autonomous action. Changes here can allow the daemon to act beyond its trust level.

### 4. autoDream Sandbox (`dream/sandbox.rs`)
Memory consolidation runs in a restricted sandbox. Changes here can allow memory consolidation to corrupt source files or execute arbitrary commands.

---

## 🧪 Known Attack Surfaces

### DREAMSWARM.md Injection
A malicious `DREAMSWARM.md` in a cloned repository could contain prompt injection instructions.
**Mitigation**: Display a warning when `DREAMSWARM.md` is loaded from an untrusted repo. Never auto-execute instructions without confirmation.

### Bash Command Injection
Bash tool validators can potentially be bypassed by creative command construction.
**Mitigation**: 25+ validators, tree-sitter AST analysis, and blocklist + allowlist + risk scoring.

### Memory Poisoning
Malicious content stored in memory could influence future agent behavior.
**Mitigation**: Skeptical memory principle—the agent verifies memory against the codebase before acting.

---

## 📦 Dependency Policy
- All dependencies are audited via `cargo-deny`.
- No `unsafe` code without explicit justification.
- No network access outside the query engine.
- No file system access outside the working directory and `~/.dreamswarm/`.
