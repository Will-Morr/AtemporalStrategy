# Lobbies and basic opponents

Requirements: [user specification](../user_spec/lobbies_and_bots.md).

## Current implementation

The public server runs as a coordinator. Each game has a stable `/games/<id>/` address, an isolated worker process and its own archive directory. The coordinator forwards WebSocket messages to loopback workers; simulation and match state remain in the worker. Workers begin at lobby creation to reuse the existing live roster and five-map picker, but simulation begins only after Start. This lifecycle is an implementation choice.

Coordinator metadata and per-game setup persist on disk. Workers resume from their most recent archive after a coordinator restart. Standalone worker mode remains available for native peripherals and existing tooling. Passwords are a casual per-game cookie gate, not authentication or encryption. Static assets remain on the public origin; each game pins its content and serves a matching guide beneath its game URL. Open/active workers restart automatically with the coordinator. Replay workers stop when their last viewer disconnects.

The directory remembers an unvalidated username using a browser cookie identity and records successful slot claims per game. Duplicate or empty names are allowed; this is not authentication. Game lobbies inherit the directory username without asking for it again. Clicking an open slot bar claims it, and the release button sits on the player’s claimed bar. The directory lists open, active and completed games. Creation presents common settings first and an advanced setup editor for the full existing configuration. New arrivals are spectators until claiming a slot; session tokens are scoped by game URL so switching between matches in one tab cannot erase another identity. Previous archived matches remain selectable as read-only replay workers.

Bots use published checkpoint states, filtered to player-visible enemies, and submit ordinary drafts through the controller's precheck, local-reference resolution and exact-state validation. Their deterministic policies establish mining and production, then choose useful intervention ticks and attack/explore. They never inspect pending opponent commits or modify simulation state, resources or units. Human/bot games wait for each human commitment; bot-only playtests start automatically and have an explicit configurable round budget to bound unattended work. Rusher builds grunts and intervenes early; Builder builds tanks and intervenes later; Raider builds grinders and tries a flank before pushing across the map.

Verification covers coordinator process isolation, passwords, multiple games and identity retention, restart, replay browsing, bot fairness/legality, and real-browser input scenarios with inspected screenshots. See the [verification record](integration-verification.md#game-coordinator-users-and-basic-bots--2026-10-24) for results and limits.

## Simple win

The game-type selector includes `simple_win`. It uses configurable scoreboard rules (five timeline wins by default), with no advancing history. Only completed structures count as buildings, including silos; construction ability is irrelevant. The first tick ending with any player lacking buildings ends the simulation, including in teams/FFA. Same-tick losses are resolved together. This cutoff overrides `stop_when_decided`, queued orders, missiles and minimum horizons. These multiplayer and completed-building details are implementation interpretations. Other game types retain their existing survival and cutoff rules.
