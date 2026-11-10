# Hosting on one small computer

The normal server remains a single coordinator with local game workers. The optional `scripts/host.py` supervisor polls `origin/main` every 60 seconds, builds a detached release with two Cargo build jobs, and keeps the previous server serving throughout fetch/build failures. No webhook, public restart endpoint, container cluster or external database is required.

## Install and start

On Linux, install Git, Python 3, Node/npm and the Rust toolchain specified by `rust-toolchain.toml` (including a C linker). Use a dedicated account with write access to its repository and state directory. Clone the repository as `/srv/atemporal/repo`, then run:

```sh
python3 /srv/atemporal/repo/scripts/host.py \
  --repo /srv/atemporal/repo --state /srv/atemporal/state --port 8080
```

For unattended operation, adapt the user, paths and PATH in [`deploy/atemporal.service`](../deploy/atemporal.service), install it in `/etc/systemd/system/`, and enable/start `atemporal.service`. Inspect `journalctl -u atemporal.service -f`. The service restarts the supervisor after failure; the supervisor restarts crashed servers. Only one supervisor may own a state directory.

Use a stable hostname and an HTTPS reverse proxy with WebSocket support pointing at port 8080; restrict the backend port to the proxy with the host firewall. Game passwords and names remain casual lobby features, not user authentication. There is no hard public resource quota or signup protection in this change; unrestricted strangers can still create workers. The expected deployment is a small group of friends. This work does not provision a remote host, DNS, TLS, firewall or access restriction.

## Updating safely

Each revision builds in `STATE/releases/<commit>`. The running binary, assets and pinned game content are not overwritten. Only a successful build creates `STATE/games/update-request`. The coordinator exits with code 75 when there are no live-game connections or game creations in progress, no simulation reported in progress, and a 30-second reconnection grace has elapsed. Admission and idle checking share a lock so a new connection cannot slip between the check and exit. Live-game spectators conservatively delay updates too; separate read-only replay viewers do not. Players should close live-game tabs when finished. No timeout forcibly interrupts a game to install an update.

The supervisor launches the candidate on the same port, checks that it stays alive and answers `/api/directory`, and records the successful revision in `STATE/current`. A startup failure returns to the previous release. This is startup recovery, not a database migration rollback; future incompatible storage changes need a migration/backup plan. Back up `STATE/games`. Existing tabs can reconnect or refresh at the same address. A server crash, manual stop or machine reboot is distinct from an update and cannot promise uninterrupted play.

Network/build failures retry on the next poll. Failed/incomplete builds are retried; a completed release has a `.ready` marker. Releases are retained to make recovery inspectable; periodically remove old worktrees with `git worktree remove` while keeping the current and any pending release. On a small host, release builds need free disk and can compete for CPU despite the two-job limit; the example service runs at lower priority. The supervisor itself is loaded at service startup: deploying changes to `host.py` requires updating the service's repository and restarting the service during idle time.

## Lobby activity and expiry

An active player is a connected human holding a slot. Bots and spectators do not make a game active. The default list hides inactive games; **Show inactive games** includes them. **Recent replays** also shows inactive completed matches for spectator review. A disconnected player's claimed slot does not count as online.

Six hours after the latest persisted game activity, the coordinator expires the entire game, stops its workers, omits it from every directory filter, and rejects game, guide and replay access. Expiry retains files on disk for recovery; it does not offer a public archive browser or delete data. Explicitly stopped matches and bot playtests still have their existing `archived` match phase and are reviewable until this separate six-hour game expiry.

Activity includes successful lobby/connection changes, accepted turns, uncommits and published simulation results. Merely fetching the directory, viewing a replay, seeking or keeping a tab open does not extend retention. Local unsubmitted edits are not sent to the server and cannot count. Activity timestamps survive restarts; records without timestamps fall back to game creation. Compatibility with older game data is not required; keep an old data copy separately if it is useful for reference. Maintenance runs every two seconds, with access checks also enforcing the deadline. Worker activity/presence status is sampled every 750 ms. These definitions and the update grace are implementation choices, not additional user requirements.
