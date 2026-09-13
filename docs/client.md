# Browser interaction plan

> Status: a revisable assistant proposal unless explicitly attributed to the user. See [user specifications](../user_spec/README.md). Imperative wording describes the current candidate design, not a locked requirement.

Use a canvas map and ordinary HTML controls around it. User visual direction: simple grayscale floor/wall terrain, with units and other gameplay objects standing out in contrast; minimalist movement, attack, projectile and explosion animations; unit health bars and facing based on last movement. Proposed art stays geometric: circles/rectangles with directional marks, small type glyphs, player colors plus owner numbers, completion bars and selection outlines. The map consumes most of the screen; bottom left shows selection, bottom center timeline, bottom right minimap. A compact top strip shows phase, selected tick, bank, readiness, score/time ratio, and simulation duration.

## Planning flow

1. Join a slot or spectate; see YAML-defined rules, FFA/team assignments, and fixed-target/lead victory condition in the lobby. Start publishes the initial full simulation.
2. Scrub the published timeline and select entities. Inspection is unrestricted, including immutable history; order entry is allowed only in the editable interval.
3. The first staged command chooses the turn's timestamp. Show a persistent marker and ghost paths/sites at that tick. Browsing other ticks does not silently change the draft timestamp.
4. Stage any allowed commands through hotkeys or visible buttons. A list shows each command, affected entity count, future-order policy/removal count, and validation status. Undo/redo and per-command deletion operate locally. Single-order mode replaces the draft command only after an explicit action.
5. A large **Commit turn** button shows command count and timestamp. Enter commits only when focus is outside text inputs and there is no unresolved placement gesture. No confirmation modal is necessary; the button itself is the explicit final action. A pass uses the same button with a clear “Pass turn” label.
6. Show accepted/waiting and simulation progress; preserve camera and inspected tick across publication, clamping to the new available range. Changed history and dormant order warnings are visible. A rejected draft stays editable.

This MVP need not live-simulate speculative drafts. Show intended orders and estimated costs without pretending they are guaranteed outcomes. New units inherit their output-bound group’s latest order at actual spawn, with the factory template as the proposed fallback. Selecting a factory exposes output tile, queue, loop toggle, stored order, output-group binding, effective newborn order, priority, paid progress and blocked status.

## Input map

All actions have clickable controls and discoverable hotkeys. Keys apply only when the game canvas/control surface has focus, never while typing. Mouse-only tile/entity selection combined with action keys supports full play; right-click is an optional convenience.

| Input | Action |
| --- | --- |
| Left click / drag | Select entity / box select |
| Shift + select | Add to selection; Shift-click selected entity toggles it |
| `0`–`9` | Select the corresponding control group |
| `Ctrl+0`–`Ctrl+9` | Stage replacement of group membership with current selection |
| `Ctrl+Shift+0`–`Ctrl+Shift+9` | Stage adding current selection to a group |
| `J`, then `0`–`9` | Stage output-group binding for selected factories |
| `J`, then `Backspace` | Stage clearing selected factories’ output-group binding |
| WASD | Pan camera |
| Middle drag / wheel | Pan / zoom map around pointer |
| Aiming via `F`, then tile click | Attack-move selected capable entities |
| `G`, then ally click | Support |
| `M`, then rectangle drag | Mine area with miners or constructors |
| `C`, then rectangle drag | Construct area |
| `B`, then `1`/`2`/`3`, then click or line drag | Place factory/turret/wall blueprints |
| `X` | Idle/stop selected entities |
| `O` | Cycle future-order policy: keep / drop all / drop configured window |
| `P`, then `1`/`2`/`3` | High/medium/low priority |
| `Q`, then roster number `1`–`7` | Append unit recipe to selected factories |
| `L` | Toggle selected factories' loop state (mixed becomes on) |
| `R`, then action key and map target | Set selected factories' stored order |
| `Delete` | Remove selected draft command or selected blueprint/queue item, depending on panel focus |
| `Ctrl+Z` / `Ctrl+Shift+Z` | Draft undo / redo |
| `Escape` | Cancel gesture/action mode; then clear selection |
| `Enter` | Commit/pass turn |
| `Space` | Toggle timeline playback |
| `,` / `.` | Step one tick backward/forward |
| `Shift+,` / `Shift+.` | Jump 100 ticks backward/forward |
| `Home` / `End` | Jump editable boundary / available end |
| `[` / `]` | Decrease/increase playback speed |
| `-` / `=` | Zoom timeline around cursor tick |
| `Alt+Left` / `Alt+Right` | Pan timeline |
| `T` | Return to draft timestamp |
| `V` | Statistics overlay |
| `?` | Hotkey/help overlay |

Queue panel supports arrow-key navigation, Delete removal, and explicit Cancel active / Replace pending controls with keyboard shortcuts displayed in context. Number keys recall groups in the default map mode; explicit building, recipe, priority and factory-binding prompts temporarily consume digits and display the pending choice. Escape returns to default group recall. Ctrl+digit stages persistent membership changes as shown above; plain group recall never consumes a turn. Minimap click selects camera center and drag pans. Area/line previews show valid and blocked tiles before finalizing. Placement enumerates tiles in deterministic coordinate order; no diagonal wall gaps unless explicitly selected.

Timeline supports click-to-seek, wheel zoom, drag-to-pan on a dedicated ruler or middle button, playback speed presets 0.25×/0.5×/1×/2×/4×/8× and direct tick entry. Use a separate playhead and draft marker; hatch immutable history. Event colors also have labels/tooltips. Timeline navigation does not consume a turn. Exact-state loading at an unsampled tick shows a brief indicator and disables dependent command staging until complete.

Selection panel displays mixed capabilities honestly: each action reports which selected entities can execute it. Groups can include buildings and mobile units without issuing invalid commands to the incompatible subset. Camera controls must remain usable while simulation runs. Do not capture browser shortcuts globally.

## Statistics and replay

Overlay graphs support metric selection, player/opponent selection, full-run or visible-range window, and tick hover. Implement simple canvas/SVG line plots; no dashboard framework required. Always show cumulative thinking-time ratio; indicate when a time penalty is enabled and show raw/adjusted scores separately. During active planning, label live timers separately from persisted committed totals.

Replay mode has a round picker that loads the published result for that historical round, with accepted inputs, no-op diagnostics and timing. It is distinct from scrubbing ticks within one result. Spectators get all inspection/playback controls and no editable draft. Provide readable connection/error state and reconnect without resetting the user's view unnecessarily.

Display constructors’ 50% mining rate in their selection details. Survival indicators show both requirements (active building and constructor/factory), with the failing condition recorded on elimination. Team colors/grouping distinguish alliances while preserving individual owner identity.

Round results show stalemate, win, or draw plus the separate stop reason, survivor list, and per-player/team score deltas. Multiple opposing survivors may score together. Team entries show surviving members and their count; lead mode shows the margin over the strongest rival and required N. Never label an inactivity/horizon stalemate as a draw, or a partial FFA win as a sole-player victory. Replay inspection presents the same survivor and team-score records as live play.

Lobby rule text states that scoreboard mode awards each resolved round, including passes and unchanged winning timelines. Show the configured target (default 5) or optional lead-N rule explicitly.

Team result text explains that survival points are awarded only if at least one player was eliminated. Stalemates always show zero score deltas. Draws show the configured no-score/all-players-score policy and actual credited players, independently of the empty survivor list.

When drafting entity orders, expose Keep future orders / Drop all future orders / Drop next W ticks. Hide or disable the window choice with an explanation when YAML has no window configured. The policy applies to the next staged command and is visibly stored on each draft entry; changing it for an existing entry updates that entry explicitly. `O` cycles the available choices without using another turn. Highlight affected future command components on the timeline, show the exact interval and count, and distinguish suppressed history from active orders. For group orders show only the selected members being removed. Undo/redo restores both replacement and suppression preview; changing the draft tick or rebasing recalculates the affected set. Commit sends the policy for authoritative resolution; stale-preview responses are discarded. A historical-round replay shows only removals made by that round.

Control-group panel shows ten slots 0–9, living member count, bound factory count, and saved order with source tick. An empty group can still receive an order for future spawns. Recalling a group selects its living members at the viewed tick and marks the order recipient as “Group N”; manually editing the selection switches the recipient to explicit units so individual overrides remain easy. Show the distinction beside the staged command: “Group N: current members + future spawns” versus “Selected units only.” The saved order is not continuously enforced: individually overridden members retain their actions until another group order arrives.

Factory binding and selection membership are separate controls. Binding output to Group N sends future spawns there even if the factory itself is selected via another group. Preview proposed membership/binding edits at the draft tick, support undo/redo, and display their command cost in single-order mode. Recalling a group while inspecting immutable history is still allowed; changing its persistent state follows ordinary editable-tick rules. Future-order previews label whole-group removals (including saved-order updates) separately from per-member delivery suppression. Historical round inspection shows the group state appropriate to that round, never current browser-local membership.

## Visual treatment

[The user’s visual requirements](../user_spec/controls_and_visuals.md#visual-design) is the visual baseline. Floor and cave walls use clearly distinct gray values with restrained detail. Proposed palette: medium-dark floor, darker rock, subtle room edges; reserve bright color/contrast for units, structures, ore, orders, selection, and combat effects. Exact colors are tuning proposals. Keep health/selection/order indicators legible over both terrain shades and distinguish teams without relying only on hue.

Every unit displays current health versus maximum health, exposing missing health as an unfilled/dim segment. Proposed building health bars use the same convention; partially constructed sites retain a separate completion indicator so investment is not mistaken for damage. Selected/hovered units can show exact values, with no need for floating damage numbers.

Units rotate toward their last successful movement direction. Interpret “last turn” as the most recent simulation movement step, so replay/scrubbing shows the orientation at the inspected tick; this is an explicit assistant interpretation of turn terminology. Retain facing while stationary, after blocked movement, and while attacking in another direction. Use a small forward notch/barrel/chevron on rotationally symmetric circles so facing is visible. Proposed initial facing points toward map center. Movement is a short linear tile-to-tile transition with minimal rotation easing, not a walking animation. Never interpolate through a wall or hide a collision; accurate tile state remains authoritative.

Attack feedback uses brief muzzle flashes or simple pulses, a small projectile dot/streak/line, and compact impact/destruction flashes or expanding rings that fade quickly. Proposed direct shots are straight and artillery uses a simple arc; weapon configuration determines visual style without engine branches on type name. Health and damage resolve at the simulation tick under the current combat proposal; projectile travel is presentation only and does not delay or change a hit. No particles requiring physics, screen shake, elaborate trails, or persistent debris are needed for this style.

Effects follow playback time, not real-time timers: pause freezes them, seeking reconstructs only effects active near the selected tick, and rewrites discard effects from superseded revisions. At high playback rates or crowded views, cap/aggregate cosmetic effects while preserving health, position, facing and event markers. Keep short-lived attacks visible between sampled snapshots using event records rather than inferring them from health differences. Static selection and exact-tick inspection stay usable with animations disabled; an optional reduced-motion toggle is a proposal.

Survival indicators can return from eliminated to alive within one replay. Show both loss and recovery transitions; units keep animating/executing orders while their owner temporarily fails survival checks. Final result/score panels use endpoint status, never an “ever eliminated” badge. Lobby displays configurable draw scoring and score-tie policy; default ties continue until a unique side qualifies. Proposed shared-victory mode can list several match winners. Draw awards must not display eliminated players as survivors. Timed-history shading does not imply a past elimination forbids later recovery.
