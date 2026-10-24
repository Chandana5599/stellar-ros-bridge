# Stellar Wave — issue backlog

These are the genuine, Wave-sized issues for the current cycle. Each is written
following `docs/issue-guide.md`. Copy the body into GitHub using the **Wave
task** template, apply the labels, and assign one contributor per issue.

Points: **Trivial = 100**, **Medium = 150**, **High = 200**.

Legend: `⭐ = good first issue`

---


> **Status:** all listed issues except W-03 (`cargo-deny`) are implemented on
> `main` (domain, lifecycle, transports, settlement client, contract, docs and
> deployment script). W-03 remains the only open backlog item.

## W-03 — Configure `cargo-deny` for licenses and advisories

**Labels:** `Stellar Wave`, `area: docs`, `complexity: trivial`
**Points:** 100

### Description
Add a `deny.toml` and a CI step so the workspace fails on disallowed licenses,
known advisories, and duplicate/banned crates.

### Requirements and context
- Allow `Apache-2.0`, `MIT`, and common permissive licenses; deny copyleft.
- Wire it into `.github/workflows/ci.yml` as a separate job.
- Definition of done: `cargo deny check` passes locally and in CI.

### Suggested execution
- Branch: `chore/cargo-deny`
- Add `deny.toml`; add a `deny` job to CI.

### Implement changes
- `deny.toml`
- CI job running `EmbarkStudios/cargo-deny-action`.

### Test and commit
- Paste local `cargo deny check` output in the PR.

### Example commit message
`chore: add cargo-deny license and advisory checks`

### Guidelines
- Assignment required before starting.
- PR description must include: `Closes #<issue_id>`.

---

