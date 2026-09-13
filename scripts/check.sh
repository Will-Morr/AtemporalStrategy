#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
CLIPPY_CONF_DIR="$PWD/crates/sim" cargo clippy -p atemporal-sim --all-targets --locked -- -D warnings
cargo test -p atemporal-sim --no-default-features --locked
npm ci --prefix client --ignore-scripts
npm run build --prefix client
cargo run --locked --quiet -p atemporal-tools -- guide --out fixtures/guide
npm test --prefix client
cargo run --locked --quiet -p atemporal-tools -- fixtures
git diff --exit-code -- schemas/contracts-v2.json client/src/contracts.generated.ts fixtures
