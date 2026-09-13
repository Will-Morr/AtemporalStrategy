# Atemporal Strategy

A robot strategy game where you can change the past and watch a different future unfold.

Build factories, mine resources, and send units into battle. Each round, everyone studies the same timeline and chooses a moment to add orders. Once all players commit, the game reruns from the earliest change. A factory built sooner—or a retreat ordered before an ambush—can change the entire battle.

The default game is 1v1, with simultaneous planning and a race to five points. There are also three- and four-player free-for-all games, teams, and a timed mode that gradually locks the past.

## Quick start

The host needs **Bash**, **Rust via rustup**, and **Node.js 22.23.2 with npm 10.9.8**. The repository selects Rust 1.97.1 automatically through `rust-toolchain.toml`. Other players only need a browser.

From the repository folder, run:

```sh
./quickstart.sh
```

This installs the browser dependencies, builds the game, and starts a fresh 1v1 server. The first build may take a few minutes.

1. Open **http://127.0.0.1:8080/**.
2. Choose a name and color, then claim a player slot.
3. Have your opponent claim the other slot and press **Start match** from the first occupied slot.

For a local test, use two tabs. To play on a LAN, your opponent opens `http://<host-ip>:8080/` on their computer. Extra players can spectate.

Stop the server with **Ctrl+C**. To use another port:

```sh
./quickstart.sh --port 8090
```

## Your first turn

You start with a miner, a constructor, and a turret. At tick 0:

1. Select the **miner**, press **M**, and drag a rectangle over nearby cyan ore.
2. Select the **constructor**, press **B**, choose a factory, and place it on clear ground. **R** rotates its output direction.
3. With the constructor selected, press **C** and drag over the factory blueprint to assign construction.
4. Select the factory blueprint, press **Q**, and queue a unit. Press **F**, then click a destination to give its newborn units an attack-move order. You can configure a factory before it is built.
5. **Commit your turn**. When both players commit, inspect the new timeline.

Scrub back to an earlier tick to change your plan. Orders in a turn share one timestamp; you can undo them until you commit. Passing without adding orders is also a valid turn.

Open **How to play** from the lobby, or visit `/guide/` on your server, for survival rules, factory queues, control groups, and unit stats.

## Useful controls

| Action | Controls |
| --- | --- |
| Select units | Click or drag a box; hold Shift to add to the selection |
| Move and fight | F, then click a destination |
| Mine / construct | M / C, then drag an area |
| Build a structure / queue units | B / Q |
| Move around the map | WASD, middle-button drag, or the minimap |
| Zoom / pan under the pointer | Mouse wheel / Shift + wheel, on the map or timeline |
| Play or pause the timeline | Space |
| Undo / redo draft orders | Ctrl+Z / Ctrl+U |
| Commit or pass / cancel a gesture | Enter / Escape |
| Statistics / keyboard help | V / ? |

## Hosting and saved games

Match settings live in [config/game.yaml](config/game.yaml); unit and building definitions live in [config/content.yaml](config/content.yaml). Change them before starting the server. The in-game guide generates its stats from the loaded content.

Games are archived under `replays/`. An ordinary launch starts a new match; resume an existing one with:

```sh
./quickstart.sh --resume <match-id>
```

The server also supports an optional inputs-only mode, where a native process on each player's machine reproduces the simulation locally. See [hosting, configuration, and peripheral setup](docs/development.md) for launch commands and archive tools.

## Development

The simulation and server are written in Rust; the browser client uses TypeScript and Canvas. Desktop Chromium is the primary tested browser. Narrow layouts also have browser coverage, with scrolling command panels.

```sh
scripts/check.sh                         # Rust checks, client build, and shared fixtures
npm run ui:install --prefix client       # install the test browser once
npm run ui:review --prefix client        # real-browser gameplay scenarios
```

- [Development guide](docs/development.md) — manual builds, server options, configuration, and tests.
- [Browser testing](docs/browser-testing.md) — screenshots, traces, and interactive review.
- [Architecture](docs/architecture.md) — deterministic simulation and timeline replay.
- [Verification results and limits](docs/integration-verification.md) — tested scenarios and performance measurements.
- [Remaining work](docs/implementation.md#remaining-review-coverage) — open review items.
