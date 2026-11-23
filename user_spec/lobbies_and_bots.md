# Game coordination, lobbies and basic AI

These user requirements extend [gameplay](gameplay.md) and [controls and visuals](controls_and_visuals.md). They supersede the original single-game server scope and setup-only-through-YAML restriction. Implementation choices remain revisable.

## Coordinator and lobbies

The launched webserver acts as a game coordinator. Players can create games and participate in multiple games in parallel. List games with filters for active games and open lobbies. Offer a simple optional lobby password; this is a casual access blocker, not a security system.

Game creation should be very simple: choose game type, configuration and map. Default to the existing 1v1 setup, while exposing the existing settings for experimentation. Players can select bot opponents. Preserve shared map selection before starting.

Joining a lobby defaults to spectator. Before starting, clicking an unclaimed slot bar claims it. The release/unclaim button belongs on the player’s claimed slot bar. The lobby UI also offers spectating running games. A separate Replays tab provides match review.

The user suggested splitting a started game into a separate process and routing its traffic directly to that process. This is a preferred direction to investigate, not a locked process lifecycle or routing contract.

## Small public server and game lifetime

Host on a small remote computer on the live Internet, expecting about four simultaneous players. Keep deployment simple; a large scalable update architecture is unnecessary.

Automatically update and restart when `main` changes, but do not restart while games are actively being played.

Use Lobbies for joining active lobbies and running games. Hide games without connected players by default and keep the option to show inactive lobbies. Use a separate Replays tab for review. Keep the five most recently completed matches across all players available regardless of age, including their complete server-side game logs. Archive incomplete games after 24 hours without activity. This replaces the six-hour expiry rule.

Backward compatibility with older game data is not required. A quick migration script is acceptable if old game data is useful for reference.

Track these requests in an ordered TODO list and update it as implementation proceeds.

## Landing page and quick play

At the bottom of the handwritten landing description, add: “Also this is open source if you want to play with the code or write a better bot”. Link “this is open source” to `https://github.com/Will-Morr/AtemporalStrategy`. Provide a WireGuard preview for review before pushing this change to main.

The public home page shows **Atemporal**, the subtitle **An RTS from the perspective of God.**, and several paragraphs of description. The user-authored description is stored in a separate editable file. Preserve its wording without permission; the user approved changing its winning target from 3 to 5. Place the six navigation buttons between the main introduction and the user’s experiment note. Do not use horizontal divider lines between landing-page sections.

Show the large buttons in this order: **Play Against Bot**, **View Lobby**, **View Replay**, **Game Docs**, **Patch Notes**, and **Code**. Code links to the project’s GitHub repository. One click creates a new lobby with the player and the default best available AI, then starts immediately without setup friction. **View Lobby** opens the game directory. **Game Docs** opens a quick explanation of the UI, command types and unit roles, including unit stats.

For this quick-play path, skip manual map/start steps. Quick play now asks first-time players to pick a username before starting (superseding the earlier decision to skip the prompt). Keep normal multiplayer lobby setup available through View Lobby.

## End-of-game feedback

Add a Give feedback button to the game-end page. Players can write a message directly to the creator. Save it persistently and link it to the specific match log. No public feedback listing or viewer is needed.

## Lightweight users

On the first visit to the game directory, let players enter a username without validation. Quick play asks for a username too. Provide a basic user identity so the coordinator can track who is playing which games. Pass this username into game lobbies without a duplicate username input. No authentication or unique-name requirement was requested; browser persistence and identity mechanics are implementation choices.

## Minimal opponents and automated playtesting

Keep the previous Standard policy selectable as bot 1.0. Quick play uses the strongest available policy. Both policies use the same player-visible information and normal orders as a human.

Review recent games actually played, find the bot’s failure modes, and implement a new opponent that aims to beat most recorded player plans when replayed. Measure its results separately from bot self-play and evaluate balance using specific unit compositions and factory loop queues with comparable matter budgets. Release balance changes supported by those comparisons.

Bot policies must have the same information a player can learn during a match. They may process an entire published timeline in one pass, keep a database of exact sightings and compute statistical features before choosing a turn. They do not need to drive the browser or match a human's scrubbing speed. Fog of war applies before data reaches a policy, including historical queries and changes in vision from allies, walls, satellites and recon. A bot may remember the last place it saw a base after it leaves vision. Later hidden changes must not update that memory until the player could observe them.

Tactical should use the published player timeline to direct troops and choose missile targets. It should remember where enemy units were observed coming from and use that evidence to estimate possible factory locations under fog.

Explore how a bot could improve through self-play and iterative parameter tuning. The choice of optimizer, opponent pool and training budget is open.

## Execution constraints

Proceed independently through implementation and verification without asking for further input. While the device is busy, keep builds and tests low priority and resource usage limited; use at most two CPU cores and run heavy checks sequentially. This temporarily supersedes the earlier six-core test allowance.

## Network and host safety

Back up game logs before deleting them from the live server. The latest user instructions narrow and supersede the earlier whole-server backup rule:

> Build out tooling in this repo to automatically back up and clean up logs. I want to have a cron job on this machine that hourly syncs all game logs to this machine, cleans up all logs on the server >24 hours old (once we've confirmed that the logs are fully synced) and clears out old builds on the server.
>
> This tooling does not need to back up server code and executables.
>
> We only care about game logs. Do not bloat this local drive with a million copies of the same unchanging folder. The game logs should include what commit the game was played on if they do not.

The hourly sync must run unattended across interactive SSH session expirations.

Only game logs (the server's game state folder) need a local copy, kept as a single incrementally updated mirror rather than repeated snapshots. Cleanup preserves the full logs for the latest five completed matches. Other eligible game logs older than 24 hours may be deleted only after their local copy is confirmed. Old builds are cleared without a backup. Each game log records the server commit it was played on. See [hosting maintenance](../docs/hosting.md#hourly-game-log-maintenance) and the [backup rule](../AGENTS.md#game-log-backup-before-live-server-cleanup).

Stop the worker process after a lobby or replay is left idle, including newly created lobbies people never join. Reopening the same URL restores the game or replay. Give briefly disconnected browsers a chance to reconnect. Continue simulations, all-bot playtests, and the bot replies owed to a human who already submitted their turn. Preserve any submitted turns when a game sleeps while waiting for another human.

Review network-facing code for unintended code execution and similar host compromise, and fix and push concrete issues found. The user is not asking for anti-cheat protections; compromise of the trusted GitHub/repository update path is an accepted attack boundary. Keep the review and fixes appropriate to the small-server design.

## Test-server opening shortcut

Provide a test-server flag that adds a homescreen button labeled **DEBUG_TEST**. It runs a bot-versus-bot opening to turn 2 and then lets the player take over, making UI changes quick to review without manually replaying the start. Ordinary servers must not show or enable this shortcut.

## Tactical openings and recovery

Tactical should contest a strong opening and recover when its first plan fails. Evaluate its opening and its subsequent interventions against the recorded human plans that beat it in effectively one turn.

Game results should arrive promptly, including while a release builds. Give live game work priority over deployment builds. Diagnose unusually slow turns against recorded matches.

Pause bot planning when everyone has left a human-versus-bot game and no human commitment is waiting. Resume when a human reconnects. Finish committed rounds and preserve unattended bot-only playtests.
