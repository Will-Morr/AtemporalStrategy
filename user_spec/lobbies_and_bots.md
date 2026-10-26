# Game coordination, lobbies and basic AI

These user requirements extend [gameplay](gameplay.md) and [controls and visuals](controls_and_visuals.md). They supersede the original single-game server scope and setup-only-through-YAML restriction. Implementation choices remain revisable.

## Coordinator and lobbies

The launched webserver acts as a game coordinator. Players can create games and participate in multiple games in parallel. List games with filters for active games and open lobbies. Offer a simple optional lobby password; this is a casual access blocker, not a security system.

Game creation should be very simple: choose game type, configuration and map. Default to the existing 1v1 setup, while exposing the existing settings for experimentation. Players can select bot opponents. Preserve shared map selection before starting.

Joining a lobby defaults to spectator. Before starting, clicking an unclaimed slot bar claims it. The release/unclaim button belongs on the player’s claimed slot bar. The lobby UI also offers spectating running games and viewing recent replays.

The user suggested splitting a started game into a separate process and routing its traffic directly to that process. This is a preferred direction to investigate, not a locked process lifecycle or routing contract.

## Lightweight users

On the first visit, let players enter a username without validation. Provide a basic user identity so the coordinator can track who is playing which games. Pass this username into game lobbies without a duplicate username input. No authentication or unique-name requirement was requested; browser persistence and identity mechanics are implementation choices.

## Minimal opponents and automated playtesting

Bots provide easy, repeatable sandbox opponents for learning without requiring another person. They must not cheat and should work through logic similar to a player. Both bot styles should be useful as opponents and as mechanics playtesters.

The latest request replaces the initial minimal opponents with **two roughly capable bots**. Both must grow an economy, produce large armies, and construct and roughly use every available unit and building (walls and silos may be omitted). They need exploration to discover player locations and should support interesting back-and-forth self-play and matches against each other. Use practical heuristics rather than ideal or solved play. Nondeterministic early aggression or caution is acceptable; specific personality names and tuning remain implementation choices.

Address economic recovery, meaningful reactions to visible enemies, timeline intervention around losses, deliberate single-order planning, and preservation of useful actions when another action fails validation. Bots must retain ordinary player visibility and order validation.

## Execution constraints

Proceed independently through implementation and verification without asking for further input. While the device is busy, keep builds and tests low priority and resource usage limited; use at most two CPU cores and run heavy checks sequentially. This temporarily supersedes the earlier six-core test allowance.
