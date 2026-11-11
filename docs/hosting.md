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

Each revision builds in `STATE/releases/<commit>`. The running binary, assets and pinned game content are not overwritten. Only a successful build creates `STATE/games/update-request`. The coordinator exits with code 75 as soon as no game creation or feedback request is in progress and no simulation reported in progress, checked every two seconds. Connected players and spectators do not hold an update back: accepted turns are on disk, and an open game tab saves its uncommitted draft in session storage and reloads itself when it sees the new server instance, restoring that draft if the round is unchanged. Admission and idle checking share a lock so a new game creation cannot slip between the check and exit.

The supervisor launches the candidate on the same port, checks that it stays alive and answers `/api/directory`, and records the successful revision in `STATE/current`. A startup failure returns to the previous release. This is startup recovery, not a database migration rollback; future incompatible storage changes need a migration/backup plan. Back up `STATE/games`. Existing tabs can reconnect or refresh at the same address. A server crash, manual stop or machine reboot is distinct from an update and cannot promise uninterrupted play.

Network/build failures retry on the next poll. Failed/incomplete builds are retried; a completed release has a `.ready` marker. Releases are retained to make recovery inspectable; periodically remove old worktrees with `git worktree remove` while keeping the current and any pending release. On a small host, release builds need free disk and can compete for CPU despite the two-job limit; the example service runs at lower priority. The supervisor itself is loaded at service startup: deploying changes to `host.py` requires updating the service's repository and restarting the service during idle time.

## Lobby activity and expiry

An active player is a connected human holding a slot. Bots and spectators do not make a game active. The default list hides inactive games; **Show inactive games** includes them. **Recent replays** also shows inactive completed matches for spectator review. A disconnected player's claimed slot does not count as online.

Six hours after the latest persisted game activity, the coordinator expires the entire game, stops its workers, omits it from every directory filter, and rejects game, guide and replay access. Expiry retains files on disk for recovery; it does not offer a public archive browser or delete data. Explicitly stopped matches and bot playtests still have their existing `archived` match phase and are reviewable until this separate six-hour game expiry.

Activity includes successful lobby/connection changes, accepted turns, uncommits and published simulation results. Merely fetching the directory, viewing a replay, seeking or keeping a tab open does not extend retention. Local unsubmitted edits are not sent to the server and cannot count. Activity timestamps survive restarts; records without timestamps fall back to game creation. Compatibility with older game data is not required; keep an old data copy separately if it is useful for reference. Maintenance runs every two seconds, with access checks also enforcing the deadline. Worker activity/presence status is sampled every 750 ms. These definitions and the update grace are implementation choices, not additional user requirements.

For counting unique users, the coordinator appends a line to `STATE/games/visits.jsonl` on each name set, game creation, and live-game or replay connection: time, event, IP (`X-Forwarded-For` from Caddy), user agent, language, identity cookie and the `atemporal-fp` browser fingerprint cookie. Pages set the fingerprint cookie from a hash of canvas/WebGL rendering, screen, timezone, languages and hardware, so it usually survives cleared cookies on the same machine. Caddy's access log on the droplet covers every page hit, including visitors who never start a game.

## Network and host boundary

Public games cannot request more simulation threads than the coordinator's local startup configuration (`match_defaults.simulation_threads`, four by default). Oversized requests are rejected before launching a worker. Persisted games above that budget are also refused; keep the host budget adequate for games you want to resume. This limits one request-driven thread-amplification path, not total worker count or total CPU use. The source review, fixed issue and remaining limits are recorded in [Network and host boundary review](security-review.md).
