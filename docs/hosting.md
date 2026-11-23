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

Network/build failures retry on the next poll. Failed/incomplete builds are retried; a completed release has a `.ready` marker. Old release worktrees are removed by the hourly maintenance job below. On a small host, release builds need free disk and can compete for CPU despite the two-job limit; the example service runs at lower priority. The supervisor itself is loaded at service startup: deploying changes to `host.py` requires updating the service's repository and restarting the service during idle time.

The supervisor passes the release commit to the server as `ATEMPORAL_COMMIT`. Each match's `manifest.json` and each round record store it as `server_commit`, so a game log shows which commit played it, including a match that spans an update. Servers started without the variable omit the field.

### Stock rules 8 migration verification

The public build at `f2454685b418de49bbd8d204396009a8640a9e87` recorded a two-player game on seed 42 using its stock content, whose canonical SHA-256 starts with `a8da7078e2f6`. A constructor moved from (6, 5) to (15, 15) by tick 30, and the enemy turret fired six times. The upgraded rules 12 release binary reproduced both recorded revisions with `--verify`, including the movement round's terminal tick 124 and hash `40061be02183…`.

Two local `--resume` runs restored the saved slots, the exact tick-30 snapshot and all six attack events. One run rebuilt the missing result cache. Both accepted a new turn and reached tick 358 with final SHA-256 `7ed85f02edd5e554200665cd6171e76d119cedc533ad3f3cf9fd6fc4f6ac5913`, identical to continuing the same game on the original engine. The pinned content and original movement-round record stayed byte-identical. All three revisions passed `--verify` after continuation. The check used standalone workers and the stock public content described in [replay compatibility](development.md#final-browser-and-integration-checks).

## Hourly game-log maintenance

[`scripts/server-maintenance.py`](../scripts/server-maintenance.py) runs on the operator's machine over SSH. The remote account needs read and cleanup access to the game state and permission to remove release worktrees as the repository owner. Each run:

1. Pulls `STATE/games` into a single local mirror (default `~/server-backups/personal-vps/games`) with rsync. Unchanged files are skipped and nothing is deleted from the mirror, so it holds one copy of every game ever played without repeated snapshots.
2. Protects the full game folders containing the five latest completed matches across all players, including rematches. It reads the winning round for logs from older servers. Selects other game folders that the directory marks expired (or no longer lists) and whose newest file is more than 24 hours old.
3. Compares every file in those folders against the mirror with an rsync checksum pass and a local size check. Any folder with a mismatch stays on the server.
4. Deletes only confirmed files whose size and modification time are unchanged since step 2, then removes empty folders. `directory.json`, `visits.jsonl` and feedback stay on the server.
5. Removes release worktrees other than the current release, the newest finished build and any build in progress. Code and executables are not backed up.

A sync or verification failure stops the run before anything is deleted. Each run appends a JSON summary to `maintenance-runs.jsonl` beside the mirror. `--dry-run` syncs and verifies but deletes nothing. `scripts/check-server-maintenance.py` covers the sequence against local directories.

Use a dedicated SSH host alias for cron. Configure it for key-based OpenSSH on the VPS's private Tailscale address, with a pinned server host key, a dedicated identity file, `IdentitiesOnly yes` and `BatchMode yes`. Scope the server-side key to the operator machine's tailnet IP and disable forwarding and TTY allocation. Interactive Tailscale SSH can keep its existing sign-in policy. Both rsync and the cleanup commands use the alias passed with `--host`.

On the live VPS, systemd starts `atemporal-backup-sshd.service` at boot. Its listener binds to the VPS's Tailscale IPv4 address on port 2222. The daemon reads `/etc/ssh/atemporal-backup/sshd_config` and `/etc/ssh/atemporal-backup/authorized_keys`. It accepts the operator machine's key as the non-root `atemporal` account. If either machine is replaced or receives a new tailnet IP, update the listener, source restrictions and local SSH configuration as applicable. Confirm the VPS host key over the administrative connection before updating the local pin.

Before switching cron, run the command below manually with `--dry-run` under the cron user's account with `SSH_AUTH_SOCK` unset. Check the newest JSON record for `ok: true`, `rsync_exit: 0`, and an empty `unverified` list. Preserve the existing mirror when changing the connection.

The operator installs the tested script outside the checkout so a branch switch cannot revert the retention policy. When updating maintenance, run `install -D -m 755 scripts/server-maintenance.py ~/.local/lib/atemporal/server-maintenance.py` from the verified revision.

Example hourly entry:

```cron
17 * * * * /usr/bin/python3 -I /home/will/.local/lib/atemporal/server-maintenance.py --host atemporal-backup >> /home/will/server-backups/personal-vps/maintenance.log 2>&1
```

System journals and Caddy logs are not part of this job; their own rotation limits apply.

## Lobby activity and expiry

Lobbies lists games with a connected human player. Show inactive games also lists open or in-progress games whose players disconnected. Finished and explicitly archived matches appear under Replays.

After 24 hours without game activity the coordinator closes the live game and stops its live worker. It persists that decision across restarts. The five most recently completed matches across all players remain available through their read-only replay URLs, with the original lobby password where applicable. Rematches count separately. The ranking uses the winning round’s original file time, including for older logs. Recently stopped unfinished matches are also reviewable for 24 hours after stopping and do not use any of the five places. The hourly job keeps the entire game folder for each protected match and checksum-verifies other eligible logs against the local mirror before removing them.

Activity includes successful lobby/connection changes, accepted turns, uncommits and published simulation results. Merely fetching the directory, viewing a replay, seeking or keeping a tab open does not extend retention. Local unsubmitted edits are not sent to the server and cannot count. Activity timestamps survive restarts; records without timestamps fall back to game creation. Maintenance runs every two seconds, with access checks also enforcing the deadline. Worker activity/presence status is sampled every 750 ms. These definitions and the update grace are implementation choices, not additional user requirements.

For counting unique users, the coordinator appends a line to `STATE/games/visits.jsonl` on each name set, game creation, and live-game or replay connection: time, event, IP (`X-Forwarded-For` from Caddy), user agent, language, identity cookie and the `atemporal-fp` browser fingerprint cookie. Pages set the fingerprint cookie from a hash of canvas/WebGL rendering, screen, timezone, languages and hardware, so it usually survives cleared cookies on the same machine. Caddy's access log on the droplet covers every page hit, including visitors who never start a game.

## Network and host boundary

Public games cannot request more simulation threads than the coordinator's local startup configuration (`match_defaults.simulation_threads`, four by default). Oversized requests are rejected before launching a worker. Persisted games above that budget are also refused; keep the host budget adequate for games you want to resume. This limits one request-driven thread-amplification path, not total worker count or total CPU use. The source review, fixed issue and remaining limits are recorded in [Network and host boundary review](security-review.md).

## Checker failure reports

A disagreement between the browser and the authoritative order checker produces a report in the affected match archive under `checker-incidents/<report-id>.json`. Search the game's worker.log for the report ID shown to the player, or the server stderr log for a standalone match. The file includes the submitted orders, player seat, intervention tick, revision, match configuration, content, rules fingerprint and exact world state used for validation. Treat the file as operator data because it includes both players’ positions. The hourly game-log mirror includes these files.
