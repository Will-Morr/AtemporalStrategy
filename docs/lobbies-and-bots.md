# Lobbies and basic opponents

Requirements: [user specification](../user_spec/lobbies_and_bots.md).

## Home page and quick play

`/` introduces **Atemporal** with the requested subtitle and user-authored description. Edit `client/landing-description.html`; the client build inserts the intro before the buttons and the experiment note after them, splitting at `<!-- BUTTONS -->`. Large buttons lead to one-click bot play, the directory at `/lobbies/`, and the explainer at `/guide/`.

Quick play names the game "<username> vs Bot v<n>", where the server counts earlier games with that name, expired ones included. It reuses a saved username or asks for one in a dialog first; browsers still named by the old `Player` default are asked again. Cancelling the dialog leaves the landing page unchanged. It copies the server's default match settings into a two-player authoritative FFA, picks the first generated map, creates a lobby, claims slot 0 through the ordinary protocol, and starts before navigating to the game. It retains the player token for refresh. A disabled button prevents repeat clicks; a pending game URL permits retry without creating another lobby. The default opponent is centralized in `bots::DEFAULT_STYLE`, Tactical 2.0.

The explainer opens with the requested chatbot-documentation warning, then gives a short UI walkthrough, command summaries and unit roles. Its static unit SVGs share `client/unit-icons.json` with the game’s selection and production controls; a visual key illustrates ore, timelines and construction previews. Full rules are expandable; unit statistics are still generated from effective server content, with game-specific guides using pinned match content.

## Current implementation

The public server runs as a coordinator. Each game has a stable `/games/<id>/` address, an isolated worker process and its own archive directory. The coordinator forwards WebSocket messages to loopback workers; simulation and match state remain in the worker. Workers begin at lobby creation to reuse the existing live roster and five-map picker, but simulation begins only after Start. This lifecycle is an implementation choice.

Coordinator metadata and per-game setup persist on disk. Workers resume from their most recent archive after a coordinator restart. Standalone worker mode remains available for native peripherals and existing tooling. Passwords are a casual per-game cookie gate, not authentication or encryption. Static assets remain on the public origin; each game pins its content and serves a matching guide beneath its game URL. Unexpired open/active workers restart automatically with the coordinator. Replay workers stop when their last viewer disconnects.

The directory remembers an unvalidated username using a browser cookie identity and records successful slot claims per game. Duplicate or empty names are allowed; this is not authentication. Game lobbies inherit the directory username without asking for it again. Clicking an open slot bar claims it, and the release button sits on the player’s claimed bar. The directory lists games with connected human players by default. Show inactive games includes disconnected/open games, and Recent replays exposes recently completed games. Creation presents common settings first and an advanced setup editor for the full existing configuration. New arrivals are spectators until claiming a slot; session tokens are scoped by game URL so switching between matches in one tab cannot erase another identity. Previous completed or manually archived matches remain selectable as read-only replay workers until the game expires after six hours without activity. Expired games are no longer viewable. See [hosting and lifetime rules](hosting.md) for activity definitions and the idle-safe main updater.

Bots choose a tick and submit an ordinary draft. The controller checks references and validates orders against the exact simulation state before committing. Both the live worker and the replay evaluator give the strategy the same published player timeline. Human/bot games wait for each human commitment; bot-only playtests start automatically and have an explicit configurable round budget to bound unattended work. Standard maintains three miners and a constructor, adds a second factory when the economy can support it, builds a defensive turret against visible nearby threats, and redirects its army. Utility production is non-looping; combat production loops. It can rewind before its published elimination instead of progressing beyond a dead base. The lobby selector preserves Standard 1.0 under its existing serialized name. Tactical 2.0 opens a dedicated combat factory, finances it with miners and establishes a separate missile silo. See the [balance analysis](balance-analysis.md) for recorded replay and simulation results.

Tactical scans the published replay for enemy units moving out of fog. It follows their observed direction back across passable terrain and counts distinct units whose routes point to the same area. Two units can justify sending troops to investigate even long after contact. An inferred missile target needs at least three units arriving through crossings at least three tiles apart within the last 600 ticks of the published replay. Tactical assigns at most one missile to that inferred area in the draft. A factory seen directly, even briefly by recon, gives Tactical a stronger target. Seeing the site empty clears that memory. Nearby armed threats take priority for troop orders, and the missile planner checks the blast area for friendly units visible at the chosen order tick.

Verification covers coordinator process isolation, passwords, multiple games and identity retention, restart, replay browsing, bot fairness/legality, and real-browser input scenarios with inspected screenshots. See the [verification record](integration-verification.md#game-coordinator-users-and-basic-bots--2026-10-24) for results and limits.

## Developing a bot

Strategy code lives in `crates/bots`. Its input is a `PolicyTimeline` scoped to one player and one published revision. It has the match rules, the editable tick range, and the player's observations. Its output is a `TurnDraft`. The server adapter owns the controller, replays the published inputs, and validates the submitted draft. Pending turns never enter the replay. To change a policy, work against the observations in this interface.

- Use `timeline().frames()` to scan the complete execution once, including ticks between display samples. Use `frame(tick)` to inspect one exact state. Observers include completed allied units, moving allied satellites and active recon zones. Public terrain and ore can be examined across the map. A teammate’s unfinished building outside shared sight remains in fog.
- Use `last_seen(tick, id)` for the latest actual sighting through that tick, with its timestamp. For example, a moving enemy last seen at tick 40 has a known location at tick 40. Its location at tick 60 can only be inferred. Use `remembered_buildings(tick)` for buildings currently in fog whose last known location has not subsequently been observed empty or occupied by a different building.
- Use `public_stats` for sampled economic and military trends. Filtering a frame to hostile factories gives an observed factory count. A player's public infrastructure-value graph describes their wider economy without disclosing where they built it. Comparing the two can help a bot decide where to scout.
- To answer a question about another published timeline, call `history().analyze(revision)` and reuse the returned database for all queries about it. Earlier revisions have their own observations and terminal ticks. The history also provides scores, outcomes and public turn markers. A policy can study later ticks of a published timeline before submitting an order at an earlier editable tick, just as a player can scrub the replay. Opposing pass turns have no public order tick.
- The browser fixtures in `fixtures/gameplay/player-visibility.json` and the Rust tests under `crates/sim/tests/player_*.rs` exercise both implementations. A human can inspect a tick exactly or play a sampled animation; batch agents can inspect every integer tick. Bot input follows active-player fog even when a spectator can watch the whole match.

### Analyze an archived match

From the repository root, export one player's timeline as newline-delimited JSON. Supply the match archive containing `manifest.json` and a zero-based player ID. The optional final argument selects a published revision; the default is the latest. The exporter reads the archive in place and checks the terminal tick and final state hash before writing output.

```sh
cargo run --release -p atemporal-server --example player_replay -- replays/MATCH_ID 0 > player-0.jsonl
```

The header contains the revision, player, rules, terrain, match result, public turn markers and the player's published commands. Each subsequent row contains one exact `frame`. The first row and each change in sight include `visible_tile_ranges` as half-open ranges of row-major map indices. An omitted mask carries the previous one forward. A `public_stats` sample appears only at its published tick. Population counts and the aggregate destruction count belong to the round comparison. Enemy missile markers contain their observed position and heading; they do not reveal a hidden launch site or destination. For example, to list each enemy factory the player observed and the first tick it was seen, stream the file once:

```python
import json

with open("player-0.jsonl") as rows:
    header = json.loads(next(rows))
    player = header["player"]
    first_seen = {}
    for line in rows:
        frame = json.loads(line)["frame"]
        for entity in frame["visible_entities"]:
            if entity["owner"] != player and entity["type_key"] == "factory":
                key = json.dumps(entity["id"], sort_keys=True)
                first_seen.setdefault(key, (frame["tick"], entity["tile"]))
    print(list(first_seen.values()))
```

This example assumes a free-for-all; team games can use the public team assignments in the header. For strategy evaluation, run the [seeded and recorded-opponent evaluator](balance-analysis.md#reproduction). Native policies can query the in-memory database directly. The export writes a frame per tick and can be large on long games.

The player-view boundary applies to the strategy inputs and archive export. The existing trusted native peripheral still receives privileged replay inputs so it can reproduce the match. Bot implementations should use the player-view interface for both live play and offline training.

## Simple win

The game-type selector includes `simple_win`. It uses configurable scoreboard rules (three timeline wins by default), with no advancing history. Only completed structures count as buildings, including silos; construction ability is irrelevant. The first tick ending with any player lacking buildings ends the simulation, including in teams/FFA. Same-tick losses are resolved together. This cutoff overrides `stop_when_decided`, queued orders, missiles and minimum horizons. These multiplayer and completed-building details are implementation interpretations. Other game types retain their existing survival and cutoff rules.

## Private end-of-game feedback

Public coordinator game-end menus include Give feedback. A message is POSTed to `/api/games/<id>/feedback` and stored as `games/<id>/replays/<match-id>/feedback/<submission-id>.json` under the coordinator data root. Records include the exact match log path, game/match IDs, viewed revision, browser identity/name, receipt timestamp and message. The UI accepts up to 5,000 characters and keeps unsent text on failure. A per-message submission ID makes retries idempotent across coordinator restarts. The write is flushed before success is reported.

Feedback has no read/list endpoint and is not included in the public directory. It remains on disk with the archived game even after public viewing expires. Existing lobby password access and a known browser identity are required; these remain casual identity/access controls, not verified accounts. The creator can inspect the JSON files directly. There is no email delivery, public inbox or standalone-worker feedback route.

## Player timeline verification

The November 18 checks cover equal observations, selected ticks and drafts after a hidden-only state change, pending opponent orders, and reading an evicted revision after a later round publishes. The satellite regression observes a building between ordinary samples, retains its last sighting through an unseen removal, then clears the remembered building when recon shows the empty tile. The [browser comparison](browser-testing.md#player-visibility-and-bot-observations) checks the browser against an export of the same archive.

In a read-only export of a recorded rules-7 match, revision 4 reproduced its terminal hash and produced 15,708 player frames (ticks 0–15,707). Exporting player 0 and consuming the JSON lines took 4.7 seconds on the development host with two-core affinity. The exporter peaked at 53.9 MiB RSS. This is one archived workload. Peak resource use at 2,000 live entities remains unmeasured. The seeded evaluator also completed a first-to-three match between Tactical and Standard on seed 42.

Replaying revisions 2, 4 and 9 of that match through both Tactical policies gave six before-and-after drafts, one per seat and revision. In revision 4 the updated policy changed five squad destinations and reduced three silo plans from three target tiles to two. In revision 9 it redirected a 61-unit group from (6, 6) to (24, 2). The recorded match supplied identical observations for each comparison. Separately, Tactical playing seat 1 against Standard won 3–0 on seeds 42 and 211 with both policies. Those matchups establish no change in win rate.
