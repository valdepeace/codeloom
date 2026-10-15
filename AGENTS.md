# AGENTS.md

Context for agents working in the `codeloom` repository. Read this before acting.

## What CodeLoom is

CodeLoom is a code agent written in **Rust** with a **ratatui** TUI, whose
orchestration model (**agent -> subagents -> jobs**, shown in the header) is
**based on deepseek-harness (dsh)**. It reuses dsh's *design* but improves
*execution* with Rust's real parallelism. It integrates optionally with `herdr`
(a terminal runtime) as a plugin, without patching it, and also runs standalone.

## Current state: design only, no code yet

There is **no `Cargo.toml`, no `crates/`, no `src/`**. Nothing builds. The
repository holds:

- `docs/specs/` — the design (base spec + per-feature idea specs).
- `docs/plans/2026-10-13-loom-mvp.md` — the **m0+m1 implementation plan**, written
  task-by-task with steps and verification. This is the entry point to start
  coding.

To bootstrap the project, follow
`docs/specs/2026-10-14-loom-self-hosting-bootstrap.md`.

## Where the truth is

- Base design: `docs/specs/2026-10-13-loom-design.md`.
- Map of all pieces: `docs/specs/2026-10-14-loom-architecture-overview.md`
  (start here to understand how things fit).
- Reference analysis of herdr/crush/dsh:
  `docs/reference/analisis-herdr-crush-dsh.md`.
- When a spec and code disagree, the spec is the intent; fix one of them
  deliberately, don't silently diverge.

## Commands

Once the workspace exists (after `docs/plans/2026-10-13-loom-mvp.md` Task 1),
these are the required checks — **all three must pass on every commit** (from the
plan's Global Constraints):

```bash
cargo build
cargo test
cargo fmt --check
cargo clippy -- -D warnings
```

The binaries are `loom` and `codeloom` (`cargo run -p codeloom-cli --bin loom`).
Toolchain is pinned by `rust-toolchain.toml` (edition 2021). They do not work yet:
the workspace does not exist.

## Architecture rules (hard)

- **`codeloom-core` must NOT depend on `ratatui` or `crossterm`.** The engine is a
  pure, testable library (spec §2). TUI, host integrations, voice, decision,
  compression live in **optional crates** (`codeloom-tui`, `codeloom-herdr`, ...), never
  in the core.
- Library errors use `thiserror`; the binary may use `anyhow`.
- Tools that write are `ExecutionMode::Exclusive` (they form a barrier); read-only
  tools are `Parallel`.
- **No real network in tests**: the provider is always a mock (`wiremock` or a
  test `Provider`).
- One binary pair, two modes: `codeloom-cli` builds both `loom` and `codeloom`,
  which autodetect standalone vs `herdr` plugin.

## Conventions

- **Language**: the specs and the README are **Spanish**; code comments and CLI
  strings follow the file you touch. Match the file, don't convert it.
- **Naming**: product **CodeLoom**; crates.io `codeloom-core` / `codeloom-cli` /
  `codeloom-tui`; binaries `loom` and `codeloom` (`loom` is the short command for
  the same brand); integration crate **`codeloom-herdr`**, plugin
  **`codeloom.herdr`**; the set of subagents is the **"flock"** shown in the header.
- Follow the patterns already written in the specs (message model, `trait
  Provider`, tool registry, event bus) rather than inventing new ones.

## Gotchas

- Don't invent build/test commands beyond the four above; they are the plan's
  contract.
- The specs are **design, not implementation**: they describe intent and open
  questions (`## Preguntas abiertas`). Don't treat an open question as settled.
- Many ideas here are deliberately **out of the MVP** (subagents/jobs = m2, herdr
  = m3, MCP/LSP/skills/memory = m4). Keep the MVP minimal; don't pull later
  features into m1.
