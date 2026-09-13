# Gameplay and match rules

These are consolidated user decisions that amend [the original prompt](core_prompt.md). Repeated confirmations are combined; later decisions supersede earlier conflicting rules. Implementation interpretations belong in `../docs/`, not in this specification.

## Opening and economy

Each player starts with a miner, constructor, and turret.

Constructors can mine at 50% of the rate of a dedicated miner. The purpose is to preserve a constructor's ability to recover the player's economy and rebuild.

Ore is scattered around the map in clusters of 1 to 9 tiles, weighted toward the smaller sizes, so the map allows back-and-forth play. There does not need to be a large guaranteed deposit next to the starting position (playtest decision, 2026-09-13; this supersedes the earlier fixed start patch).

Map generation should be fresh for every game (or at least drawn from a pool of randomly generated maps). Ore must not concentrate along the main corridors: that forces constant unit collisions as troops push past constructors and makes the corridor the only tactically significant place. Distribute ore better, tending to place it along the edges of open areas. Ore must not sit on the outside edge of intersections where armies collide with miners; isolated rooms should get ore. The ideal is smaller rooms with ore in them while big rooms only have ore in tucked-away corners. Generate in two passes: cave generation, then ore placement driven by a pathing heatmap from about 100 random path pairs (axis or diagonal pathing, about a quarter weighted toward opposite corners to simulate relative pathing), with a raw heatmap and a smoothed heatmap whose fixed gradient keeps ore from concentrating one tile off the major highways. Place ore in low-traffic places, somewhat evenly spaced from existing veins, and never on extremely high-traffic paths (map decision, 2026-09-13).

The map centre must always be passable. Most maps otherwise form two matching routes and which one units take is pseudorandom; the main path should always go through the centre, with additional winding paths available off to the sides (map decision, 2026-09-13). The layout itself should not be one large open area with a few insignificant columns and a handful of dead-end tunnels (map decision, 2026-09-13).

A miner claims the tile it is mining. Other miners sent to the same area path through or around claimed tiles instead of all converging on the closest one, so a group fills out a fillable area rather than clustering on its entry corner (playtest decision, 2026-09-13).

## Elimination and simulation outcomes

A player loses if they have **no active building OR no ability to build**. Active buildings currently mean turrets and factories. No ability to build means having no constructors or factories.

The objective is to have more simulations end in actual wins/losses, without rewarding players for hiding random cheap units to force draws in games they have clearly lost. The user's recovery rationale was “any player that has at least one constructor is still in the game”; the explicit loss predicate above also requires an active building. Keep that distinction visible rather than silently changing OR to AND. Unfinished entities do not prevent elimination: only completed entities satisfy the survival requirements.

In multiplayer, units persist after their player becomes eliminated and continue their orders. Elimination can reverse during the same simulation: a constructor can complete a factory while a teammate keeps the game going, restoring the player. A recovered player should not be considered eliminated. Results and survival scoring therefore use the status at the end, not whether the player ever temporarily failed a survival check.

Simulations have three result types:

- **Stalemate:** both players remain alive in 1v1.
- **Win:** a player was eliminated and another survives; multiplayer can have multiple scoring survivors.
- **Draw:** all players were eliminated.

## Simultaneous turns

All players plan against the same published timeline. Reveal their orders and rerun the simulation for everyone only after all players commit. This was explicitly confirmed after the initial request.

## Timed mode

Advance the immutable history boundary by a fixed, configurable number of ticks per completed planning round.

Timed adjudication considers only the constructor/factory end state: the game is decided when locked history shows a player with no constructors or factories. Loss of all active buildings while a constructor still exists is not sufficient for timed finalization. Exact multiplayer team aggregation remains an explicit implementation interpretation.

## Hybrid mode

Add a hybrid mode with the scoreboard goal of five timeline wins and a fixed advance of immutable history each round.

## Dynamic simulation horizon

Use a configurable inactivity duration to stop dynamically based on lack of order progress or unit destruction, with an ambitious absolute tick limit as a backup.

Remove the elimination-based early-stop shortcut. Continuing after elimination permits both recovery and draws when persisting units eliminate the remaining players. Do not stop just because zero or one side currently satisfies the survival checks.

Playtest amendment (2026-09-13): the simulation must not keep running long after a 1v1 is effectively over. At the very least, stop when one side has literally no units remaining, or is in a losing (eliminated) state with no active or queued orders. An eliminated side that still has a unit carrying an order, or a command scheduled at a later tick, keeps the run going so recovery and mutual elimination remain possible. Applying this per side in team and FFA play is an implementation interpretation; the stop is a configurable match rule (`stop_when_decided`), on by default.

The user's precise wording and reasoning were:

> The simulation horizon should be able to be handled dynamically by stopping if no order has progress made on it or no unit is destroyed for some amount of time. As games have strictly finite numbers of units that can be built and orders are only given by players, infinite sim runs should be impossible by this ruleset. Some ambitious absolute tick limit is probably still useful.

What counts as progress, the exact boolean condition, and how future scheduled inputs affect stopping remain implementation interpretations; this consolidation does not lock the assistant's proposed answers.

## Scoreboard and teams

Award points each resolved round, including passes: an unchanged winning timeline continues to score. Default match victory is first to **5 points**, configurable.

Multiplayer supports **FFA or team play**. Add an optional dynamic score-to-win configuration requiring one player to have **N more points than every other player**, since several players may score together. Team scores are based on the number of their members still alive after the simulation.

**Only award survival points when at least one player is eliminated at the end.** Temporary elimination followed by recovery does not count as an elimination for this condition. Stalemates never score. Draw scoring is configurable: either no players score or all players score. This draw-specific rule supersedes the earlier non-scoring-draw assumption; it does not change survival-based scoring for wins.

Game-end score ties must be configurable. The user specified the default as “have play continue until there is a single team with a score exceeding the win state.” The current architecture interprets this as one uniquely leading team meeting the configured victory threshold, preserving the earlier first-to-5 rule; alternate tie policies and exact threshold comparisons remain explicit implementation details.

There is no scoreboard round limit. Players can stop and archive manually; repeated stalemates or tied scores do not cause an automatic administrative result.

The original request's optional time-based score penalty remains in scope. Team application of lead-N, exact penalty formulas, and other unconfirmed details remain proposals in the architecture documents.

## Server launch and restart

The game process must accept a command-line port argument. Restarting it on the same port should let all players return by refreshing their existing browser tab. Startup/restore behavior should preserve that stable address; specific CLI naming and default port remain implementation choices.

Provide a quickstart script that builds and launches the default 1v1 server.

## Allied traffic and deadlocks

The user expects a very simple mechanism to resolve unit deadlocks, such as allowing a unit to enter a teammate bot's occupied tile and moving the displaced bot by swapping positions or pseudorandomly placing it on an adjacent tile. Prefer such lightweight mechanics over a complex crowd/pathfinding system. Exact displacement precedence, legal moves and deterministic arbitration remain implementation proposals.


Constructors must not remain parked on a factory output indefinitely blocking production. Attack-moving armed units must respond to enemies firing on them when their capabilities permit, including grunt/turret encounters.

## Uncommitting while waiting

Players may uncommit a submitted turn while waiting for the other players, recover their moves for editing, and commit again. The turn still closes immediately when all players commit; uncommit is unavailable once simulation starts. This should encourage early commitment without penalizing someone who immediately notices a missing action.
