# Atemporal Strategy

A small browser-based robot RTS where players rewrite orders in the past and inspect the resulting future, with a deterministic Rust simulation and server.

With Rust/rustup, Node 22.23.2 and npm 10.9.8 installed, build and launch the default 1v1 game:

```sh
./quickstart.sh
```

Open `http://127.0.0.1:8080/` in two tabs, claim one slot per tab, and press **Start match**. Stop the server with **Ctrl+C**. Use `./quickstart.sh --port 8090` for another port. The script installs locked browser dependencies, builds the client and release server, and uses `config/game.yaml`; Rust's pinned toolchain is selected by `rust-toolchain.toml`. See [development](docs/development.md) for configuration, multiplayer, peripheral and archive usage, and the [review checklist](docs/implementation.md#remaining-review-coverage) for remaining coverage. The original request is [user_spec/core_prompt.md](user_spec/core_prompt.md).

Read the plan in this order:

1. [Decisions and scope](docs/decisions.md) — confirmed requirements and revisable implementation choices.
2. [Architecture and simulation](docs/architecture.md) — time travel, deterministic execution, thread jobs, and performance.
3. [Data contracts](docs/contracts.md) — shared types, protocol, persistence, and ownership.
4. [Browser experience](docs/client.md) — controls, timeline, statistics, and accessibility.
5. [Implementation and agent handoffs](docs/implementation.md) — nested checklist, worktrees, integration gates, and verification.

Only user decisions are locked. Read [the user specification record](user_spec/README.md) for authoritative input. All architecture choices and defaults below are revisable proposals. Remaining implementation interpretations are labeled in the decision register. Build dependencies and toolchains are pinned. See [development and launch](docs/development.md), the [stabilized v2 contracts](docs/contracts-v2.md) and the [Gate 2 measurements](docs/gate2-measurements.md).

For iterative visual checks from any compatible agent, use the [Playwright review harness and browser MCP setup](docs/browser-testing.md).
