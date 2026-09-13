# Atemporal Strategy

A small browser-based robot RTS where players rewrite orders in the past and inspect the resulting future. The simulation and server will be written in Rust.

The coordinator foundation is implemented: versioned Rust/TypeScript contracts, causal identities, round scoring, shared content validation, generated unit guide, browser build scaffold, and tiny-world acceptance fixtures. The simulation and game server are the next single-lead integration slice; this is not yet a playable game. The original request is [user_spec/core_prompt.md](user_spec/core_prompt.md).

Read the plan in this order:

1. [Decisions and scope](docs/decisions.md) — confirmed requirements and revisable implementation choices.
2. [Architecture and simulation](docs/architecture.md) — time travel, deterministic execution, thread jobs, and performance.
3. [Data contracts](docs/contracts.md) — shared types, protocol, persistence, and ownership.
4. [Browser experience](docs/client.md) — controls, timeline, statistics, and accessibility.
5. [Implementation and agent handoffs](docs/implementation.md) — nested checklist, worktrees, integration gates, and verification.

Only user decisions are locked. Read [the user specification record](user_spec/README.md) for authoritative input. All architecture choices and defaults below are revisable proposals. Remaining implementation interpretations are labeled in the decision register. Build dependencies and toolchains are now pinned. See [development commands](docs/development.md) and the [provisional v2 contracts](docs/contracts-v2.md).

For iterative visual checks from any compatible agent, use the [Playwright review harness and browser MCP setup](docs/browser-testing.md).
