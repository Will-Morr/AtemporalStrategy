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

The browser build regenerates JSON Schema, TypeScript declarations, and guide artifacts. These generated sources are checked in for subsystem handoffs. Review their diffs alongside contract/content edits. `client/dist` is disposable build output. The static preview is local-only and contains no game transport:

```sh
PORT=8090 npm run dev --prefix client
```

Open `http://127.0.0.1:8090/`; the guide is `/guide/`. An occupied port fails instead of choosing another address. The future actual server launch remains `atemporal-server --port 8090 --config config/game.yaml`, with `--resume <match_id>` for archived matches; that binary/CLI is server-agent work and is not available yet.

Useful implemented Rust commands:

```sh
cargo run --locked -p atemporal-tools -- schema
cargo run --locked -p atemporal-tools -- normalize --setup config/teams.yaml --out target/normalized-teams
cargo run --locked -p atemporal-tools -- guide --content config/content.yaml --out target/custom-guide
cargo run --locked -p atemporal-tools -- select-guide --content config/content.yaml --bundled client/public/guide --cache .guide-cache
```

For an archived content file, add `--expected-hash <manifest content_hash>` to `select-guide`. It rejects mismatching saved data. The selected directory must be mounted by the server before the match bootstrap publishes its guide URL. The reusable library is `atemporal-content`; these commands exercise the same loader and fallback.

See [contracts v1](contracts-v1.md) for encoding, scoring interpretations, subsystem ownership, and version-change rules. See [fixtures](../fixtures/README.md) for golden-world acceptance and its current limits.
