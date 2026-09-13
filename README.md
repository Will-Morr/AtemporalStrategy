# Atemporal Strategy

A small browser-based robot RTS where players rewrite orders in the past and inspect the resulting future. The simulation and server will be written in Rust.

The vertical slice is implemented: a deterministic Rust simulation, a server that runs it on a dedicated thread with durable turns and results, and a browser client that plays the opening, commits simultaneous turns, seeks any tick and rewrites history. It is an engineering milestone (Gate 2), not the complete planned game; breadth work on the roster, variants, graphs, replay and the peripheral continues before the first playtest. Launch it with the commands in [development](docs/development.md). The original request is [user_spec/core_prompt.md](user_spec/core_prompt.md).

Read the plan in this order:

1. [Decisions and scope](docs/decisions.md) — confirmed requirements and revisable implementation choices.
2. [Architecture and simulation](docs/architecture.md) — time travel, deterministic execution, thread jobs, and performance.
3. [Data contracts](docs/contracts.md) — shared types, protocol, persistence, and ownership.
4. [Browser experience](docs/client.md) — controls, timeline, statistics, and accessibility.
5. [Implementation and agent handoffs](docs/implementation.md) — nested checklist, worktrees, integration gates, and verification.

Only user decisions are locked. Read [the user specification record](user_spec/README.md) for authoritative input. All architecture choices and defaults below are revisable proposals. Remaining implementation interpretations are labeled in the decision register. Build dependencies and toolchains are pinned. See [development and launch](docs/development.md), the [stabilized v2 contracts](docs/contracts-v2.md) and the [Gate 2 measurements](docs/gate2-measurements.md).

For iterative visual checks from any compatible agent, use the [Playwright review harness and browser MCP setup](docs/browser-testing.md).
