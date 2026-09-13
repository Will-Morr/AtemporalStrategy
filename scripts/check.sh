#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
npm ci --prefix client --ignore-scripts
npm run build --prefix client
npm test --prefix client
cargo run --locked --quiet -p atemporal-tools -- fixtures
git diff --exit-code -- schemas/contracts-v1.json client/src/contracts.generated.ts client/public/guide fixtures
