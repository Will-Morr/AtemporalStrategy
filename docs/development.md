# Development and launch

The vertical slice is implemented: a deterministic simulation library, a server that runs it on a dedicated thread, and a browser client that plays the opening, commits simultaneous turns, seeks any tick and replays. Pinned tools: Rust 1.97.1 (`rust-toolchain.toml`), Node 22.23.2 (`.node-version`), npm 10.9.8. Direct dependencies use exact versions with lockfiles for the rest. `serde_yaml` 0.9.34 is a pinned deprecated parser for this prototype.

## Launch

```sh
npm ci --prefix client            # once
npm run build --prefix client     # regenerates schema/types and bundles client/dist
cargo run --release -p atemporal-server -- --port 8080 --config config/game.yaml
```

Open `http://127.0.0.1:8080/` in two browser tabs (or two machines on the LAN), claim slot 0 and slot 1 with a username and color, and press **Start match** from the first occupied slot. Extra tabs spectate. The guide is `/guide/`, generated at startup from the loaded content into `.guide-cache/<content hash>/`. `--port` beats the `PORT` environment variable, which beats `default_port` in the YAML; an occupied port fails with a message instead of choosing another address. Other flags: `--content`, `--client`, `--prose`, `--replays`, `--guide-dir`, `--help`.

Refreshing a tab reconnects with the slot token kept in browser storage and receives the current revision and planning round. Restarting the server starts a fresh match at the same address; resume from an archive is server breadth work.

Each match writes `replays/<match-id>/`: `manifest.json`, pinned `config.yaml`/`content.yaml`, `initial-state.json`, `turns/<round>-<player>.json` (flushed before the commit is acknowledged), `rounds/<round>.json` (outcome, score, hashes, command outcomes, sim time), regenerable `results/<revision>/` caches and `measurements.jsonl`.

## Checks

```sh
scripts/check.sh                  # fmt, workspace tests, strict clippy, client build, fixture drift
node scripts/gate2-check.mjs      # real server + protocol: three rounds, rewrite, seeks, sizes → target/gate2-summary.json
ATEMPORAL_UI_SERVER_COMMAND='cargo run --release -q -p atemporal-server -- --replays target/ui-replays' \
  npm run ui:review --prefix client -- --grep slice --project=desktop-chromium
```

The workspace tests include the engine acceptance suite: all eight authored tiny worlds run through the real engine and are checked with the golden comparator, checkpoint reruns hash-match full replays, and the generated map runs the miner → factory → grunt → attack opening (`crates/sim/tests`). The browser walkthrough (`client/tests/ui/slice.spec.mjs`) drives two players and a spectator with real input against the real server and keeps screenshots under `artifacts/ui/<run>/`. Both are engineering verification, not a user playtest. See [Gate 2 measurements](gate2-measurements.md) for the recorded numbers and [browser review](browser-testing.md) for the harness.

Useful tools: `cargo run -p atemporal-tools -- schema | guide | normalize | fixtures`. Regenerate authored tiny-world data deliberately with `fixtures`; it writes expectations, not engine output.

## Layout

- `crates/contracts` — versioned records, identities, locks, scoring, timed adjudication, golden comparator.
- `crates/content` — content/setup loading and validation; optional `guide` feature.
- `crates/sim` — the engine: `world.rs` indexes, `fields.rs` flow fields, `commands.rs`, `tick.rs`, `output.rs`, `map.rs` (2-player fixture map). Clippy forbids HashMap/HashSet here.
- `crates/server` — `adapter.rs` sim thread, `controller.rs` match state, `archive.rs` files, `ws.rs` routes/protocol, `main.rs` CLI.
- `client/src` — `net.ts`, `lobby.ts`, `game.ts` (state, input, panels), `render.ts` (map, minimap, timeline).
- `crates/runner` — reserved for the native peripheral; still a placeholder.
