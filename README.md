# Atemporal Strategy

A small browser-based robot RTS where players rewrite orders in the past and inspect the resulting future. The simulation and server will be written in Rust.

This repository currently contains the **architecture pass**, not a playable implementation. The original request is [user_spec/core_prompt.md](user_spec/core_prompt.md).

Read the plan in this order:

1. [Decisions and scope](docs/decisions.md) — proposed rules and decisions requiring user input.
2. [Architecture and simulation](docs/architecture.md) — time travel, deterministic execution, processes, and performance.
3. [Data contracts](docs/contracts.md) — shared types, protocol, persistence, and ownership.
4. [Browser experience](docs/client.md) — controls, timeline, statistics, and accessibility.
5. [Implementation and agent handoffs](docs/implementation.md) — nested checklist, worktrees, integration gates, and verification.
6. [Adversarial review](docs/review.md) — risks and acceptance scenarios.

Only user decisions are locked. Read [the user specification record](user_spec/README.md) for authoritative input. All architecture choices and defaults below are revisable proposals. Unanswered product questions are marked in the decision register; they do not block unrelated work. No external dependencies or current package versions are selected by this planning pass.
