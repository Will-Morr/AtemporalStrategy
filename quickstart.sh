#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")"

echo "Installing browser dependencies and building the client..."
npm ci --prefix client --ignore-scripts
npm run build --prefix client

echo "Building and starting the default 1v1 server."
echo "Open the server's printed URL in two tabs, claim one slot per tab, then Start match."
echo "Press Ctrl+C to stop. Extra arguments are passed to the server (for example: --port 8090)."
exec cargo run --release --locked -p atemporal-server -- --config config/game.yaml "$@"
