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
