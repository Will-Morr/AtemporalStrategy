# Gameplay and match rules

These are consolidated user decisions that amend [the original prompt](core_prompt.md). Repeated confirmations are combined; later decisions supersede earlier conflicting rules. Implementation interpretations belong in `../docs/`, not in this specification.

## Opening and economy

Each player starts with a miner, constructor, and turret.

Constructors can mine at 50% of the rate of a dedicated miner. The purpose is to preserve a constructor's ability to recover the player's economy and rebuild.

## Elimination and simulation outcomes

A player loses if they have **no active building OR no ability to build**. Active buildings currently mean turrets and factories. No ability to build means having no constructors or factories.

The objective is to have more simulations end in actual wins/losses, without rewarding players for hiding random cheap units to force draws in games they have clearly lost. The user's recovery rationale was “any player that has at least one constructor is still in the game”; the explicit loss predicate above also requires an active building. Keep that distinction visible rather than silently changing OR to AND. Whether unfinished buildings count as active has not been specified.

Simulations have three result types:

- **Stalemate:** both players remain alive in 1v1.
- **Win:** a player was eliminated and another survives; multiplayer can have multiple scoring survivors.
- **Draw:** all players were eliminated.

## Simultaneous turns

All players plan against the same published timeline. Reveal their orders and rerun the simulation for everyone only after all players commit. This was explicitly confirmed after the initial request.

## Timed mode

Advance the immutable history boundary by a fixed, configurable number of ticks per completed planning round.

## Dynamic simulation horizon

Use a configurable inactivity duration to stop dynamically based on lack of order progress or unit destruction, with an ambitious absolute tick limit as a backup.

The user's precise wording and reasoning were:

> The simulation horizon should be able to be handled dynamically by stopping if no order has progress made on it or no unit is destroyed for some amount of time. As games have strictly finite numbers of units that can be built and orders are only given by players, infinite sim runs should be impossible by this ruleset. Some ambitious absolute tick limit is probably still useful.

What counts as progress, the exact boolean condition, and how future scheduled inputs affect stopping remain implementation interpretations; this consolidation does not lock the assistant's proposed answers.

## Scoreboard and teams

Award points each resolved round, including passes: an unchanged winning timeline continues to score. Default match victory is first to **5 points**, configurable.

Multiplayer supports **FFA or team play**. Add an optional dynamic score-to-win configuration requiring one player to have **N more points than every other player**, since several players may score together. Team scores are based on the number of their members still alive after the simulation.

**Only award survival points when at least one player was eliminated.** Stalemates award none. All-eliminated draws also award none because no players survive.

The original request's optional time-based score penalty remains in scope. Team application of lead-N, simultaneous fixed-target ties, exact penalty formulas, and other unconfirmed details remain proposals in the architecture documents.
