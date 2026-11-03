# DreamSwarm remediation plan

All work is proposed, not executed. The [review](REVIEW.md) identifies the pinned evidence and limitations. Do not weaken thresholds or delete tests to pass a gate. Observed paths below refer to the reviewed revision; proposed paths are explicitly labeled.

## Dependency map and phase gates

`DS-T1 → {DS-T2, DS-T3} → DS-T4 → DS-T5 → DS-T6 → DS-T7`. T2 covers execution security and data; T3 covers confirmed API/correctness findings. T4 covers supply-chain automation and consumes the T2/T3 regression tests, so it follows both rather than running independently. T5 needs all three; T6 needs a candidate release and the security fixes; T7 independently verifies the whole chain. A failed gate stops promotion, not evidence collection.

| Phase | Entry | Exit |
|---|---|---|
| Baseline / reproduction | Isolated workspace approved | T1 records call paths, historical versus reproduced results and unknowns |
| Defects and risk-based security | T1 plus owner threat model | T2/T3 demonstrate actual boundaries and truthful responses |
| Meaningful automation | T1; T2/T3 tests available | T4 gates the candidate, including negative controls |
| Deployment and recovery | T2–T4 accepted; owner objectives approved | T5 observes immutable release, rollback and restore |
| UX and independent acceptance | T5 candidate available | T6/T7 record supported journeys, independent evidence and exceptions |

### DS-T1 — Establish baseline and trace paths (DS-3, DS-5, DS-7)

**Prerequisites:** owner approves an isolated workspace and non-production credentials. **Observed paths:** `.github/workflows/ci.yml`, `.github/workflows/verification.yml`, `Cargo.lock`, `src/api/server.rs`, `src/runtime/agent_loop.rs`, `src/dream/sandbox.rs`, `src/swarm/executors/`.
1. Record revision, toolchains, supported platforms and the snapshot's historical run links separately from new results.
2. In an isolated environment run existing workflow checks, including the already-defined focused locked tests, and retain failures and missing prerequisites.
3. Trace each actual CLI, daemon, API and executor entry through permission, sandbox, tool and event-consumer paths; identify which sandbox applies and whether dashboard control signals have consumers.

**Deliverable:** baseline and call graph with trust boundaries. **Verification:** a second reviewer traces one user-triggered action to its effect and reproduces one representative check.
- [ ] Candidate SHA, environment, failures and unrun checks recorded.
- [ ] Sandbox and non-sandbox paths, DS-7 consumers and unknown wiring distinguished.

### DS-T2 — Enforce execution and data boundaries (DS-3, DS-4)

**Prerequisites:** DS-T1; owner-approved permitted workspaces, credentials and threat model. **Observed paths:** `src/runtime/permissions.rs`, `src/tools/bash_tool.rs`, `src/dream/sandbox.rs`, `src/swarm/executors/`, `src/db/`, `src/memory/`.
1. Compare each execution path's actual permission decisions, OS isolation, path handling, environment inheritance and network access; account for existing controls before designing changes.
2. Close verified gaps at the real enforcement points using least privilege and explicit approval; separately establish storage classification, access, redaction and retention.
3. Add independent adversarial fixtures for denied tools, shell variations, symlink/path escapes, prompt-driven misuse, credential leakage and cost exhaustion; include allowed operations to detect overblocking.

**Deliverable:** implemented boundary matrix and regression tests. **Verification:** in a disposable environment deliberately disable one selected enforcement check and observe the corresponding negative test fail; restore and rerun, exercising the real entry path.
- [ ] Each supported executor has documented and observed authorization and isolation behavior.
- [ ] Secret and persistent-data handling has tested owner-approved limits; unresolved paths remain blocked.

### DS-T3 — Protect API credentials and make control results honest (DS-6, DS-7)

**Prerequisites:** DS-T1; owner chooses whether mutually untrusted local users are in scope. **Observed path:** `src/api/server.rs`; dashboard asset `static/index.html`. **Proposed path:** `tests/api_security.rs` if no existing appropriate suite covers the boundary.
1. Model local clients, Host/Origin checks, bootstrap, logs, bearer lifetime and stream credentials; preserve legitimate local dashboard and API flows.
2. Remove credential logging and implement an owner-approved authenticated bootstrap and separately scoped stream access; specify failure behavior and revocation.
3. Connect control endpoints to observed action acknowledgments or return an honest accepted/unavailable state; expose failures to the UI rather than claiming execution.

**Deliverable:** reviewed API trust model, corrected handlers and tests. **Verification:** exercise unauthenticated, wrong-token, cross-origin, stream-token reuse and authorized requests; inject a missing/failing action consumer and assert the response cannot claim confirmed execution.
- [ ] Unauthorized local access and token leakage are tested against the approved trust model.
- [ ] Control status matches observed action or explicitly reports uncertainty/failure.

### DS-T4 — Make automation and dependencies trustworthy (DS-1; preventive)

**Prerequisites:** DS-T1 and T2/T3 regression tests. **Observed paths:** `.github/workflows/ci.yml`, `.github/workflows/verification.yml`, `.github/workflows/dependency-audit.yml`, `Cargo.lock`.
1. Pin `typos@master` and other third-party actions to verified full commits; document ownership and reviewed update cadence.
2. Exercise existing locked verification and dependency-audit checks in a controlled environment; investigate advisories and tool provenance without suppressing findings.
3. Put boundary and control tests in required checks; verify branch protection and failure propagation with authorized repository access.

**Deliverable:** action inventory, advisory decisions and enforceable CI gates. **Verification:** deliberately break a relevant behavior and confirm its test and required workflow fail; restore and rerun. Separately inspect that pinned references match reviewed upstream commits.
- [ ] Action provenance and dependency decisions recorded.
- [ ] Required checks reject a demonstrated regression without lowered thresholds.

### DS-T5 — Release and recovery (DS-2, DS-4)

**Prerequisites:** T2–T4; owner approves registry, signing and recovery objectives. **Observed paths:** `.github/workflows/docker.yml`, `.github/workflows/release.yml`, `Dockerfile`; **proposed path:** `docs/operations/recovery.md`.
1. Define immutable revision/version identifiers, digest-based deployment, artifact provenance and signing policy.
2. Release an approved staging candidate, record its digest and restart the daemon with representative non-production persisted data.
3. Back up and restore that data to an isolated instance, measure recovery, then roll back to a previously recorded digest.

**Deliverable:** release and recovery runbook with observed results. **Verification:** independent observer checks the running digest and restored data, not just workflow exit codes.
- [ ] Immutable artifact and rollback are observed.
- [ ] Restore meets owner-approved objectives or the exception blocks promotion.

### DS-T6 — Usability and accessibility (DS-7; preventive)

**Prerequisites:** T3 corrected behavior and T5 staging candidate. **Observed paths:** `src/tui/`, `static/index.html`, `docs/index.html`.
1. Determine which terminal, dashboard and documentation surfaces are supported and shipped.
2. Test keyboard operation, focus, terminal alternatives, contrast and readable error/approval states with representative users.
3. Repair confirmed issues and document unsupported clients and accessible alternatives.

**Deliverable:** surface-specific usability findings and fixes. **Verification:** independently repeat a dashboard control journey and a terminal journey, including denied and failed actions.
- [ ] Applicable accessibility issues have owners and dispositions.
- [ ] UI states reflect real action outcomes and supported journeys are documented.

### DS-T7 — Independent acceptance (preventive)

**Prerequisites:** T2–T6 complete and candidate SHA frozen. **Observed paths:** CI workflows, affected implementation and tests; **proposed path:** `docs/operations/acceptance.md`.
1. An independent reviewer maps each finding to the changed code, live entry point and regression evidence.
2. Repeat authorization, deliberately broken negative-control, recovery and release-digest checks against the candidate; record unrun checks and failures.
3. Present unresolved risks and evidence to the owner for explicit acceptance or rejection.

**Deliverable:** signed-off acceptance record tied to candidate SHA. **Verification:** reviewer checks artifacts and observed behavior independently; test counts alone are not proof.
- [ ] Each finding is resolved or explicitly excepted with owner and expiry.
- [ ] Acceptance evidence is reproducible and covers the deployed revision.

## Production acceptance, approvals and rollback

- [ ] Owner approves workspace scope, local-user trust, provider credentials, retention, deployment platform, registry, signing and recovery objectives.
- [ ] Candidate passes phase gates and independent checks; all exceptions and unrun checks are visible.
- [ ] Release only the reviewed immutable digest; stop promotion on failed security or recovery checks. Roll back to the last verified digest and restore tested data if necessary, documenting any data-loss window.

Changes to exposure, permissions, data deletion/retention, credential handling, signing, release or backup policy require owner approval. Commands described here are future workflow actions, not actions performed in this review.
