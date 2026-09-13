# Coordinator development and handoff

The coordinator implementation provides contracts, scoring, content, guide generation, and a browser build scaffold. It is not yet a playable game. Simulation, server, and runner crates reserve subsystem boundaries; the runner exits with an explicit unimplemented message.

Pinned tools: Rust 1.97.1 (`rust-toolchain.toml`), Node 22.23.2 (`.node-version`), npm 10.9.8. Direct dependencies use exact versions, with Cargo/npm lockfiles pinning transitive dependencies. `serde_yaml` 0.9.34 is a pinned deprecated upstream parser for this prototype; replacing it is revisable and must retain normalized fixture compatibility. No network is needed to regenerate guides once build dependencies are cached.

From the repository root:

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all --check
npm ci --prefix client
npm run build --prefix client
npm test --prefix client
```

The browser build regenerates JSON Schema and TypeScript declarations. The preview server generates its guide once at startup from effective content. These generated sources are checked in for subsystem handoffs. Review their diffs alongside contract/content edits. `client/dist` is disposable build output. The static preview is local-only and contains no game transport:

```sh
PORT=8090 npm run dev --prefix client
```

Open `http://127.0.0.1:8090/`; the guide is `/guide/`. An occupied port fails instead of choosing another address. The future actual server launch remains `atemporal-server --port 8090 --config config/game.yaml`, with `--resume <match_id>` for archived matches; that binary/CLI is server-agent work and is not available yet.

Useful implemented Rust commands:

```sh
cargo run --locked -p atemporal-tools -- schema
cargo run --locked -p atemporal-tools -- normalize --setup config/teams.yaml --out target/normalized-teams
cargo run --locked -p atemporal-tools -- guide --content config/content.yaml --out target/custom-guide
```

For an archived content file, add `--expected-hash <manifest content_hash>` to `guide`. It rejects mismatching saved data. The generated directory must be mounted by the server before the match bootstrap publishes its guide URL. The reusable library is `atemporal-content`; these commands exercise the same loader and startup generator.

See [provisional contracts v2](contracts-v2.md) for encoding, scoring interpretations, subsystem ownership, and version-change rules. See [fixtures](../fixtures/README.md) for golden-world acceptance and its current limits.


## Validation record

Coordinator worktree: `/home/will/atemporal-coordinator`, branch `coordinator/contracts-v1`.

- Rust workspace tests, strict Clippy, and formatting pass. They cover score rules/idempotent last-round retry, identity/canonical state hashing, malformed boundaries, shared content/setup validation, startup guide overrides/resume/corruption, fixture inputs and the golden comparator.
- Browser TypeScript checking and esbuild build pass. JavaScript validates 26 shared fixtures and malformed cases, checks tuple records, and sends serialized fixtures back through Rust for equality checks.
- Eight authored tiny worlds define quiet stopping, mining ratio, future-input guards, mutual elimination, factory recovery, dormant IDs, group birth inheritance, and partial group locks. Only input/expectation consistency and the comparator are tested here; actual engine execution, checkpoint/parallel replay equivalence, and performance remain simulation handoff work.
- Local HTTP smoke checks on port 8097 returned the scaffold, JavaScript, guide, content JSON, and manifest with expected content types. Missing routes returned 404; an occupied port failed with EADDRINUSE. The session’s built-in UI browser was unavailable; the project now supplies real Playwright browser review and independent MCP smoke verification.
- All four Chromium UI cases pass across desktop and narrow viewports, covering keyboard navigation, guide content, layout overflow, and isolated browser contexts. Screenshots were inspected. The vendor-neutral MCP smoke client also passes tool discovery, navigation, link interaction, rendered snapshot inspection, and screenshot capture.
- Runtime routing, durable score application/crash recovery, real server/simulation-thread execution, complete player controls, and end-to-end playtests remain their separate checklist tasks. The guide prose is a baseline for the client handoff, to be reviewed against actual controls.

Run `scripts/check.sh` for the complete reproducible coordinator check (including generated-artifact drift). Regenerate authored tiny-world data deliberately with `cargo run --locked -p atemporal-tools -- fixtures`; this writes expectations from `crates/tools/src/fixtures.rs`, not results from a hidden engine.

See [agent-neutral browser review](browser-testing.md) for setup, commands, screenshot/trace review, isolated sessions, MCP configuration, and the current host’s local automation-browser sandbox setting.
