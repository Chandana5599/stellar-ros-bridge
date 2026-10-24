# Contributing to Stellar ROS Bridge

Thanks for helping connect robots to the Stellar network. This project
participates in the **Stellar Wave** program, but the rules below apply to
every contribution.

## Code of conduct

By participating you agree to our [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md).

## Ways to contribute

- Pick up an open issue labeled **`Stellar Wave`** (during a Wave) or
  **`good first issue`**.
- Improve documentation and examples.
- Report bugs and propose features using our issue templates.

## Before you start

1. **Get assigned.** Comment on the issue and wait for a maintainer to assign
   you. Unassigned PRs may be closed to keep Waves fair.
2. **One issue per PR.** Keep changes focused; split unrelated work into
   separate PRs.
3. **Read the issue fully.** It contains scope, acceptance criteria, and
   suggested execution. If something is ambiguous, ask on the issue before
   writing code.

## Development setup

Requirements:

- Rust via [rustup](https://rustup.rs) — the toolchain in `rust-toolchain.toml`
  is installed automatically.
- For `ros-bridge` work: a working **ROS 2** install (Jazzy or newer) sourced
  in your shell so `rclrs` can find it.
- For `stellar-bridge` work: a Stellar **testnet** account and a funded key.
  Never commit keys — use environment variables (see `.env.example`).

```bash
cargo build --workspace
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

> Windows note: the default toolchain uses the MSVC linker. If `link.exe` is
> unavailable, either install the Visual Studio C++ Build Tools or use a GNU
> toolchain, e.g. `rustup toolchain install stable-x86_64-pc-windows-gnu` and
> run `cargo +stable-x86_64-pc-windows-gnu build`.

## Branch and commit conventions

- Branch name: `type/short-description`, e.g. `feat/task-escrow-contract`.
- Use [Conventional Commits](https://www.conventionalcommits.org/):
  `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`.
- Reference the issue in the PR body with `Closes #<issue_id>`.

## Quality bar

We review for correctness, clarity, and safety. A PR is ready when:

- The code compiles with no warnings (`-D warnings`).
- New logic has tests. Security- or money-touching code (escrow, signing,
  settlement) needs explicit test coverage for edge cases and failure modes.
- Public items are documented; non-obvious parts have short comments.
- Documentation and `CHANGELOG`-worthy behavior changes are reflected in
  `docs/`.
- The PR description states what changed, why, and how it was verified
  (commands + output where relevant).

## Review process

Maintainers aim to review within a Wave's timeframe. Expect at least one round
of feedback. Reviews may cover design, error handling, and on-chain safety.

## Security

Do **not** open a public issue for vulnerabilities. Contact the maintainers
privately (see `SECURITY.md`) so a fix can be prepared first.

## License

By contributing, you agree your contributions are licensed under
**Apache-2.0** as described in [`LICENSE`](LICENSE).

## Common developer commands

Install [just](https://just.systems/) before using these shortcuts. Run commands from the repository root.

| Command | Purpose |
| --- | --- |
| `just build` | Build the workspace. |
| `just test` | Run workspace tests. |
| `just lint` | Run Clippy for all workspace targets and deny warnings. |
| `just fmt` | Check Rust formatting without changing files. |
| `just docs` | Generate workspace documentation without dependencies. |
| `just deny` | Run `cargo deny check` (requires `cargo-deny` and its configuration). |
| `just clean` | Remove generated Cargo build artifacts. |

The recipes wrap the corresponding Cargo commands so contributors can use one consistent entry point.
