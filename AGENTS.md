# AGENTS.md

Context for agents working in the `codeloom` repository. Read this before acting.

## What CodeLoom is

CodeLoom is a code agent written in **Rust** with a **ratatui** TUI, whose
orchestration model (**agent -> subagents -> jobs**, shown in the header) is
**based on deepseek-harness (dsh)**. It reuses dsh's *design* but improves
*execution* with Rust's real parallelism. It integrates optionally with `herdr`
(a terminal runtime) as a plugin, without patching it, and also runs standalone.

## Current state: implemented and in use

The workspace builds and has tests. Crates: `batten` (TUI widgets/state),
`codeloom-core` (engine), `codeloom-a2a` (A2A wire edge, card, JSON-RPC client),
`codeloom-tui`, `codeloom-herdr`, `codeloom-cli`. Subagents run as A2A tasks; remote
A2A agents can be consumed (A8a, with client-side auth A8d) and CodeLoom can be served as a
read-only A2A agent (A8b). Streaming over SSE (A8c, serving and consuming) and the automatic agent
router (A12b, rule plus optional classifier) are done, as is the remote-agents closure (A8d-2:
OAuth2/OIDC, mTLS and card-signature checks; remote `input-required`; live cards). The status of
every A2A task is in `docs/plans/2026-10-22-codeloom-a2a-roadmap.md`.

Every task needs its spec (`docs/specs/`) and plan (`docs/plans/`) before code.
`docs/plans/2026-10-13-loom-mvp.md` is the original m0+m1 plan (history).

## Where the truth is

- Base design: `docs/specs/2026-10-13-loom-design.md`.
- Map of all pieces: `docs/specs/2026-10-14-loom-architecture-overview.md`
  (start here to understand how things fit).
- Reference analysis of herdr/crush/dsh:
  `docs/reference/analisis-herdr-crush-dsh.md`.
- When a spec and code disagree, the spec is the intent; fix one of them
  deliberately, don't silently diverge.

## Commands

These are the required checks — **all four must pass on every commit** (from the
plan's Global Constraints):

```bash
cargo build
cargo test
cargo fmt --check
cargo clippy -- -D warnings
```

The binaries are `loom` and `codeloom` (`cargo run -p codeloom-cli --bin loom`).
Toolchain is pinned by `rust-toolchain.toml` (edition 2021).

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
- **Nothing unbounded reaches the model**: every tool output, prompt section and loop has a
  cap (spec `docs/specs/2026-10-22-codeloom-decalogo-de-contexto-y-coste.md`). Read the
  decalogue before adding or changing a tool.
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
