# Game coordination, lobbies and basic AI

These user requirements extend [gameplay](gameplay.md) and [controls and visuals](controls_and_visuals.md). They supersede the original single-game server scope and setup-only-through-YAML restriction. Implementation choices remain revisable.

## Coordinator and lobbies

The launched webserver acts as a game coordinator. Players can create games and participate in multiple games in parallel. List games with filters for active games and open lobbies. Offer a simple optional lobby password; this is a casual access blocker, not a security system.

Game creation should be very simple: choose game type, configuration and map. Default to the existing 1v1 setup, while exposing the existing settings for experimentation. Players can select bot opponents. Preserve shared map selection before starting.

Joining a lobby defaults to spectator. Before starting, clicking an unclaimed slot bar claims it. The release/unclaim button belongs on the player’s claimed slot bar. The lobby UI also offers spectating running games and viewing recent replays.

The user suggested splitting a started game into a separate process and routing its traffic directly to that process. This is a preferred direction to investigate, not a locked process lifecycle or routing contract.

## Small public server and game lifetime

Host on a small remote computer on the live Internet, expecting about four simultaneous players. Keep deployment simple; a large scalable update architecture is unnecessary.

Automatically update and restart when `main` changes, but do not restart while games are actively being played.

Hide games with no active player from the default lobby list. Provide a “Show inactive games” checkbox. After six hours with no new moves or other game activity, archive the game and make it no longer viewable. Recently completed games must remain available for spectator review before expiry.

Backward compatibility with older game data is not required. A quick migration script is acceptable if old game data is useful for reference.

Track these requests in an ordered TODO list and update it as implementation proceeds.

## Landing page and quick play

The public home page shows **Atemporal**, the subtitle **An RTS from the perspective of God.**, and several paragraphs of description. The user-authored description is stored in a separate editable file. Preserve its wording without permission; the user approved changing its winning target from 3 to 5. Place the three navigation buttons between the main introduction and the user’s experiment note. Do not use horizontal divider lines between landing-page sections.

Place large navigation buttons at the bottom, with **Play Against Bot** first. One click creates a new lobby with the player and the default best available AI, then starts immediately without setup friction. **View Lobby** opens the game directory. **Game Docs** opens a quick explanation of the UI, command types and unit roles, including unit stats.

For this quick-play path, skip the previous first-visit username prompt and manual map/start steps. Keep normal multiplayer lobby setup available through View Lobby.

## End-of-game feedback

Add a Give feedback button to the game-end page. Players can write a message directly to the creator. Save it persistently and link it to the specific match log. No public feedback listing or viewer is needed.

## Lightweight users

On the first visit to the game directory, let players enter a username without validation. Quick play bypasses this prompt. Provide a basic user identity so the coordinator can track who is playing which games. Pass this username into game lobbies without a duplicate username input. No authentication or unique-name requirement was requested; browser persistence and identity mechanics are implementation choices.

## Minimal opponents and automated playtesting

Bots provide easy, repeatable sandbox opponents for learning without requiring another person. They must not cheat and should work through logic similar to a player. Several simple bot varieties are useful both as opponents and as mechanics playtesters.

Sophisticated planning and strong play are unnecessary. The suggested approach is a small opening action set, followed by a simple filter choosing a timeline intervention tick and queuing attacks there. Exact policies and bot personalities are implementation choices.

## Execution constraints

Proceed independently through implementation and verification without asking for further input. While the device is busy, keep builds and tests low priority and resource usage limited; use at most two CPU cores and run heavy checks sequentially. This temporarily supersedes the earlier six-core test allowance.
