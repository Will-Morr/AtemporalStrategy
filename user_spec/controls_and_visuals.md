# Orders, control groups, and visual design

These consolidated user decisions extend [the original prompt](core_prompt.md). They describe required behavior; architecture details and defaults remain revisable unless the user has decided them.

## Future-order replacement

When issuing new orders to units that already have future orders, players must have an option to drop all those future orders.

A configuration option must also enable dropping orders within a certain time range after the current chosen tick, allowing players to rewrite behavior over a following span of ticks.

Exact interval endpoints, affected command categories, and the removal representation have not been specified by the user. The original requirement for undoable drafts until commit still applies.

## Control groups

Provide control groups bound to keys **0–9**. Pressing a number selects that group's units.

Factories can be bound to a control group. All newly spawned units automatically join that group and inherit the most recent order sent to it.

Players can still give individual orders to units belonging to a group. A subsequent group order overrides an individual order. Save group orders for future newly built members, but treat each group order as a **single sent command for living members**, not as a continuously enforced order.

Group overlap, membership-edit gestures, factory-template precedence, and other unconfirmed details remain implementation proposals.

## Visual design

Use a simple terrain map with different shades of gray for floor and walls. Everything else should stand out in comparison.

Keep movement and attack animations minimal, including minimalist projectile and explosion animations.

Units have health bars showing damage and rotate to indicate the direction they moved last turn. The meaning of “last turn” in playback and exact interpolation/effect timing remain implementation interpretations. The original minimalist circles/rectangles and player-color approach still applies.

## Player guide and lobby

Provide a concise, readable documentation page explaining how to play, how the game works, all game mechanics, and readable unit stats. The user's preferred implementation is a static site generated at compile time. Unit stats should reference the same source of truth as the game process to prevent drift.

Link the guide from the landing page where players select teams. Players enter a username and choose a color. During setup, all usernames and player colors must be clearly visible and update for everyone as they change, so the full lobby state is known before starting.

## Timeline planning and movement performance

Players must be able to watch the replay starting from any tick and inject orders on any tick. Do not restrict order entry to sampled snapshot ticks; exact state at an arbitrary tick must remain available.

Movement must use a lightweight shared approach instead of per-unit A* searches. Destination flow fields are the accepted implementation proposal. In-process simulation, ore-budget tuning, in-world order locks, an end-to-end slice before parallel subsystem work, and one startup guide-generation path are accepted proposals; they remain implementation choices rather than additional locked gameplay rules.

## Final UI playtest requirements

The battlefield must remain selectable to every map edge, clear of the interface. Shift+wheel/pinch pans the map view and scrolls the timeline under the pointer; ordinary zoom stays anchored under the pointer. R rotates factory placement. Undo and redo use Ctrl+Z and Ctrl+U (native equivalents/compatible additional shortcuts are welcome).

A replacement constructor order must clear the earlier action visually and mechanically. Hide commands incompatible with the selection. Factories receive starting orders through the same gestures as units. Ghost buildings must be selectable and configurable before construction: production queue, priority, and newborn starting order take effect immediately on completion.

Show stored matter clearly at the viewed tick. Timeline markers include every tick with an authoritative applied order affecting any selected unit. Unit vision determines battlefield visibility with fog of war even though timeline inspection may disclose other information.

Hide full-health bars. Use distinct layered geometric silhouettes with readable direction: square factories, circular turrets on square bases, and distinguishable combinations for mobile units. Group related input buttons in left-aligned columns. Emphasize construction/production discovery and key game information over secondary diagnostics. Show unmistakable color-coded win/loss results and clearly explain continued simulation after elimination and playback after match end.

Complete and commit these fixes before implementing multiplayer, single-order, and recovery browser coverage. The user requested a separate implementation branch and worktree, and completion for real players without an intermediate human-playtest gate.


## Follow-up playtest usability

Reduce the bottom command deck height by 30%, replace dense diagnostic text with readable summaries, and make destructive/clearing buttons state exactly what they remove. Show single-unit stats in a corner and icons for the current selection. Factories, including unfunded blueprints and construction sites, need visible queue icons/counts, priority and loop status, straightforward queue removal, and clear newborn orders. Looping must be configurable before completion. Replay speed must be visibly adjustable.

Refresh/rejoin must visibly identify the player and preserve independent identities in multiple windows. Never briefly reveal enemy blueprints while seeking. Replace the activity-filled timeline with one bar divided horizontally by player, marking every exact command tick and highlighting each player's most recently written tick. Retain selected-recipient tick highlights.

H adds the selection to a group; provide an explicit clear-group action. This supersedes the previous H-replace/Shift+H-add gesture.


## Replay during simulation

Allow players to view and scrub the completed portion of a replay while the simulation is still running. The early ticks they want to inspect may finish within half a second even when the long tail takes several more seconds. The user subsequently requested implementing this feature.
