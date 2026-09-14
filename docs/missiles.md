# Missile silos

Silos use the existing matter-priority and production machinery, including per-item loop flags and blueprint configuration. A silo has no physical output tile requirement. A completed recipe adds one missile to inventory, advances that queue item's occurrence and requeues it when its own loop flag is enabled. Inventory counts remain with the silo; stock and active production are lost and accounted as invested matter when it dies. Already launched missiles continue independently.

## Current content defaults

Matter costs follow the user’s balance update; other values are implementation choices, revisable through content. User requirements are in [gameplay](../user_spec/gameplay.md#missile-silos).

| Item | Matter | Behavior |
| --- | ---: | --- |
| Silo | 300 | 300 HP; produces 2 matter/tick; launches at most once every 3 ticks |
| Satellite | 50 | Vision radius 9 during flight, then 100 ticks at its destination |
| Cluster | 125 | 100 damage to every entity within radius 7; 40% of a default tank's 250 HP |
| Tac nuke | 300 | Annihilates every entity within radius 4 |

All missiles fly at 3 tiles/tick, using ceiling-rounded Euclidean distance and a 1–30 tick flight duration. They ignore terrain and have no launch range limit. Manual targets must be map coordinates, including rock tiles. Explosions affect allies as well as enemies. Terrain rock, ore and unfunded blueprint plans are not physical entities and are unchanged by impacts.

A manual launch plan fires in list order as matching inventory becomes available. Cancelled launches leave stock intact. Automatic launching only acts with an empty manual queue, choosing the nearest visible hostile entity within 36 tiles, with entity ID breaking ties; it consumes the first available missile type in the canonical inventory order. Friendly/team vision and satellite coverage supply visibility. Automatic launching starts off, so production alone stockpiles. Priority Off disables spending, independently of launching stored missiles.

The 36-tile placement ring represents automatic acquisition, while manual launches remain unlimited, following the user's final range instruction. Flight estimates explicitly start at launch: construction, production, earlier queued launches and launch cooldown can add waiting time. The UI exposes stock, build queue and launch queue separately, with a labeled expandable production section.

## Simulation and replay

`TypeKind::Missile` distinguishes recipes that produce inventory from physical unit recipes. `TypeDefinition.missile` defines effects/flight parameters and `TypeDefinition.silo` defines automatic targeting/cooldown. Silo recipes must reference missiles; ordinary factory recipes must reference units. `Production.silo` stores counts, the pending plan, next launch tick and a monotonic launch sequence. `set_silo_plan` replaces the pending manual list and automatic setting without changing inventory. Blueprint settings carry the same plan until the site materializes. Like other production settings, these edits retain scheduled future settings.

After matter allocation, complete missile recipes enter inventory and eligible launches are collected in canonical entity order. Combat and due missile impacts resolve through the same simultaneous damage phase. Tactical nuclear impacts explicitly annihilate affected entities, including when several healers contribute simultaneously. Missiles already in flight delay elimination/inactivity termination until their impacts have resolved. Ready queued launches respect launch cooldowns even with a short inactivity cutoff; a silo with a funded or fundable launch plan remains able to act after losing its constructor. Satellite destination coverage begins in the state after impact and expires after its content-defined duration.

World checkpoints and compact samples carry active flights and reconnaissance zones; silo sample entities carry inventory/plan state, so counts remain available during sampled playback. Flight identities combine silo identity and launch sequence. Canonical ordering, bounds checks and rules stamp 6 protect checkpoint replay and controller/peripheral agreement. Active flights, landing coverage and effects are rendered from the viewed revision, including progressive replay; enemy entities retain ordinary fog filtering. Friendly missile impacts can be animated through fog without revealing entities.

## Verification

Simulator scenarios cover storage without output space, looping, Off spending, delayed launches, exact blast radii/damage, friendly fire, satellite flight/destination expiry, unlimited range and the 30-tick cap, silo death with an in-flight missile, shared-vision automatic targeting, plan cancellation and blueprint configuration. Checkpoint suffix replay is compared against full runs.

The real-browser scenario builds/configures a silo blueprint, queues all three missiles, previews turret/silo reach and missile impact areas, removes a launch, toggles automatic targeting, commits, views all missile effects, rewrites the plan into a stockpile, refreshes and launches stored ammunition. It runs with either authoritative or native-peripheral transport on desktop/narrow Chromium. See the final audit in [integration verification](integration-verification.md) for actual completed runs and visual evidence.
