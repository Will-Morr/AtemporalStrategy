# Development and launch

The vertical slice is implemented: a deterministic simulation library, a server that runs it on a dedicated thread, and a browser client that plays the opening, commits simultaneous turns, seeks any tick and replays. Pinned tools: Rust 1.97.1 (`rust-toolchain.toml`), Node 22.23.2 (`.node-version`), npm 10.9.8. Direct dependencies use exact versions with lockfiles for the rest. `serde_yaml` 0.9.34 is a pinned deprecated parser for this prototype.

## Launch

```sh
npm ci --prefix client            # once
npm run build --prefix client     # regenerates schema/types and bundles client/dist
cargo run --release -p atemporal-server -- --port 8080 --config config/game.yaml
```

Open `http://127.0.0.1:8080/` in two browser tabs (or two machines on the LAN), claim slot 0 and slot 1 with a username and color, and press **Start match** from the first occupied slot. Extra tabs spectate. The guide is `/guide/`, generated at startup from the loaded content into `.guide-cache/<content hash>/`. `--port` beats the `PORT` environment variable, which beats `default_port` in the YAML; an occupied port fails with a message instead of choosing another address. Other flags: `--content`, `--client`, `--prose`, `--replays`, `--guide-dir`, `--help`.

Refreshing a tab reconnects with the slot token kept in browser storage and receives the current revision and planning round. Restarting the server without flags starts a fresh match at the same address. `--resume <match-id>` reopens an archive under its pinned `config.yaml`/`content.yaml` (the CLI config files are ignored): lobby profiles and slot tokens come back, so open tabs refresh and reconnect; the current revision loads from `results/` or is regenerated from the ledger and hash-checked; a partial round resumes planning with its accepted turns, and a fully committed round without a record is simulated again. `--verify <match-id>` replays every round from the initial state, prints per-round hash comparisons and exits non-zero on a mismatch. A resumed archive refuses to run when the recorded content/config hashes differ.

Each match writes `replays/<match-id>/`: `manifest.json`, pinned `config.yaml`/`content.yaml`, `initial-state.json`, private `lobby.json` (slot tokens), `turns/<round>-<player>.json` plus `.request` (flushed before the commit is acknowledged; thinking time is persisted with each turn and round), `rounds/<round>.json` (outcome, score, timed adjudication, hashes, command outcomes, coarse timeline index, sim time; round 0 is the opening), regenerable `results/<revision>/` caches, `archive.json` after a manual stop or history exhaustion, and `measurements.jsonl`.

Retention: revision bodies (samples, checkpoints, stats, events, timeline) stay in memory up to `--memory-budget-mb` (default 512, JSON-size proxy), evicted least-recently-used but never the current revision; `results/` is kept under `--results-budget-mb` (default 2048), removing the oldest caches first and never turns or round records. A query on an evicted revision reloads from disk or replays the ledger on the sim thread (about 1 s for a 20,000-tick revision on the Gate 2 machine) and writes the cache back. The sim thread flushes batches by estimated bytes (4 MiB) as well as ticks, so the bounded channel caps in-flight memory. `get_stats` buckets server-side (latest sample per bucket, at most 2,000 buckets) and the published timeline index holds at most 500 entries per player.

Failure injection for Gate 5 checks: `ATEMPORAL_FAIL_AT=after_turn_written|during_job|after_results|before_publish` aborts the process at that point; `ATEMPORAL_DISK_FULL_AFTER=<n>` makes every durable write after the n-th fail like a full disk. A failed publish reopens the round with the last revision intact; the durable turns replay on `--resume`.

## Checks

```sh
scripts/check.sh                  # fmt, workspace tests, strict clippy, client build, fixture drift
node scripts/gate2-check.mjs      # real server + protocol: three rounds, rewrite, seeks, sizes → target/gate2-summary.json
node scripts/match-check.mjs      # timed lock advancement + mid-match resume + history_exhausted, timed loss, time penalty, stop/archive, occupied port/routes/restart instance/archived-content guide → target/match-check-summary.json
node scripts/gate5-check.mjs      # failure injection: kills at four points, duplicate commits, disk-full; each recovers the baseline hash → target/gate5-summary.json
ATEMPORAL_UI_SERVER_COMMAND='cargo run --release -q -p atemporal-server -- --replays target/ui-replays' \
  npm run ui:review --prefix client -- --grep slice --project=desktop-chromium
```

The workspace tests include the engine acceptance suite: all eight authored tiny worlds run through the real engine and are checked with the golden comparator, checkpoint reruns hash-match full replays, and the generated map runs the miner → factory → grunt → attack opening (`crates/sim/tests`). The browser walkthrough (`client/tests/ui/slice.spec.mjs`) drives two players and a spectator with real input against the real server and keeps screenshots under `artifacts/ui/<run>/`. Both are engineering verification, not a user playtest. See [Gate 2 measurements](gate2-measurements.md) for the recorded numbers and [browser review](browser-testing.md) for the harness.

Useful tools: `cargo run -p atemporal-tools -- schema | guide | normalize | fixtures`. Regenerate authored tiny-world data deliberately with `fixtures`; it writes expectations, not engine output.

## Layout

- `crates/contracts` — versioned records, identities, locks, scoring, timed adjudication, golden comparator.
- `crates/content` — content/setup loading and validation; optional `guide` feature.
- `crates/sim` — the engine: `world.rs` indexes, `fields.rs` flow fields, `commands.rs`, `tick.rs`, `output.rs`, `map.rs` (2-player fixture map). Clippy forbids HashMap/HashSet here.
- `crates/server` — `adapter.rs` sim thread (run/exact/replay jobs), `controller.rs` match state, lobby, retention, resume and replay verification, `archive.rs` files, `ws.rs` routes/protocol, `main.rs` CLI.
- `client/src` — `net.ts`, `lobby.ts`, `game.ts` (state, input, panels), `render.ts` (map, minimap, timeline).
- `crates/runner` — reserved for the native peripheral; still a placeholder.
