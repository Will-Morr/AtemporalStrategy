# Lobbies and basic opponents

Requirements: [user specification](../user_spec/lobbies_and_bots.md).

## Home page and quick play

`/` introduces **Atemporal** with the requested subtitle and user-authored description. Edit `client/landing-description.html`; the client build inserts the intro before the buttons and the experiment note after them, splitting at `<!-- BUTTONS -->`. Large buttons lead to one-click bot play, the directory at `/lobbies/`, and the explainer at `/guide/`.

Quick play names the game "<username> vs Bot v<n>", where the server counts earlier games with that name, expired ones included. It reuses a saved username or asks for one in a dialog first; browsers still named by the old `Player` default are asked again. Cancelling the dialog leaves the landing page unchanged. It copies the server's default match settings into a two-player authoritative FFA, picks the first generated map, creates a lobby, claims slot 0 through the ordinary protocol, and starts before navigating to the game. It retains the player token for refresh. A disabled button prevents repeat clicks; a pending game URL permits retry without creating another lobby. The default opponent is centralized in `bots::DEFAULT_STYLE`, Tactical 2.1.

The explainer opens with the requested chatbot-documentation warning, then gives a short UI walkthrough, command summaries and unit roles. Its static unit SVGs share `client/unit-icons.json` with the game’s selection and production controls; a visual key illustrates ore, timelines and construction previews. Full rules are expandable; unit statistics are still generated from effective server content, with game-specific guides using pinned match content.

## Current implementation

The public server runs as a coordinator. Each game has a stable `/games/<id>/` address, an isolated worker process and its own archive directory. The coordinator forwards WebSocket messages to loopback workers; simulation and match state remain in the worker. Workers begin at lobby creation to reuse the existing live roster and five-map picker, but simulation begins only after Start. This lifecycle is an implementation choice.

Coordinator metadata and per-game setup persist on disk. Workers resume from their most recent archive after a coordinator restart. Standalone worker mode remains available for native peripherals and existing tooling. Passwords are a casual per-game cookie gate, not authentication or encryption. Static assets remain on the public origin; each game pins its content and serves a matching guide beneath its game URL. Unexpired open/active workers restart automatically with the coordinator. Replay workers stop when their last viewer disconnects.

The directory remembers an unvalidated username using a browser cookie identity and records successful slot claims per game. Duplicate or empty names are allowed; this is not authentication. Game lobbies inherit the directory username without asking for it again. Clicking an open slot bar claims it, and the release button sits on the player’s claimed bar. The directory lists games with connected human players by default. Show inactive games includes disconnected/open games, and the separate Replays tab lists recent recordings. Creation presents common settings first and an advanced setup editor for the full existing configuration. New arrivals are spectators until claiming a slot; session tokens are scoped by game URL so switching between matches in one tab cannot erase another identity. The latest five completed matches remain available for read-only replay even after their live lobbies close. Incomplete games close after 24 hours without activity. See [hosting and lifetime rules](hosting.md) for activity definitions and the idle-safe main updater.

Replay cards show the total published rounds after the opening and the cumulative adjusted score for each player or team. In modes without points, the card shows the confirmed winner or draw when the recording provides one.

The selectable **Enclosure** bot applies a hand-written opening based on three human games. It starts the economy and constructor production near home, then uses public terrain to put turrets, walls and mixed-unit factories along two approaches. It reserves a travel corridor and factory output tiles, and the forward factories send artillery, tanks and other combat units toward the public opposing start. Its silo can send recon before Tactical's later scouting-based decisions. It falls back to Tactical's normal development when it cannot place enough forward factories or the match uses single-order turns. Quick play continues to use Tactical.

Bots choose a tick and submit an ordinary draft. The controller checks references and validates orders against the exact simulation state before committing. Both the live worker and the replay evaluator give the strategy the same published player timeline. Human/bot games wait for each human commitment; bot-only playtests start automatically and have an explicit configurable round budget to bound unattended work. Standard maintains three miners and a constructor, adds a second factory when the economy can support it, builds a defensive turret against visible nearby threats, and redirects its army. Utility production is non-looping; combat production loops. It can rewind before its published elimination instead of progressing beyond a dead base. The lobby selector preserves Standard 1.0 under its existing serialized name. Tactical 2.1 opens a dedicated combat factory, finances it with miners and establishes a separate missile silo. See the [balance analysis](balance-analysis.md) for recorded replay and simulation results.

Tactical scans the published replay for enemy units moving out of fog. It follows their observed direction back across passable terrain and counts distinct units whose routes point to the same area. Two units can justify sending troops to investigate even long after contact. An inferred missile target needs at least three units arriving through crossings at least three tiles apart within the last 600 ticks of the published replay. Tactical assigns at most one missile to that inferred area in the draft. A factory seen directly, even briefly by recon, gives Tactical a stronger target. Seeing the site empty clears that memory. Nearby armed threats take priority for troop orders, and the missile planner checks the blast area for friendly units visible at the chosen order tick.

Verification covers coordinator process isolation, passwords, multiple games and identity retention, restart, replay browsing, bot fairness/legality, and real-browser input scenarios with inspected screenshots. See the [verification record](integration-verification.md#game-coordinator-users-and-basic-bots--2026-10-24) for results and limits.

In games with humans, bot planning pauses when all human seats have disconnected and no human turn is committed for the round. A five-second grace period lets brief reconnects keep their in-progress plan. Reconnecting starts planning again automatically. Human commitments still trigger the remaining bot work and ordinary round simulation. Bot-only playtests continue without connected players.

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

## Tactical self-play

Tactical already maps a published player timeline to ordinary turn orders. The offline learner preserves that interface and evaluates changes to Tactical parameters without changing the live preset.

* **Opponent selection.** In MicroRTS, Moraes and colleagues improved the search for scripted strategies by selecting opponents that helped distinguish candidate strategies. AlphaStar also kept historical policies and trained opponents to exploit weaknesses. A small frozen pool would let us detect cycles such as a rush beating an economic opening that beats a defensive policy that stops the rush. Save the matchup matrix for every candidate. [MicroRTS study](https://www.ijcai.org/proceedings/2023/539) · [AlphaStar](https://storage.googleapis.com/deepmind-media/research/alphastar/AlphaStar_unformatted.pdf)
* **Numeric tuning.** Stockfish uses simultaneous perturbation stochastic approximation (SPSA). Increase or decrease several normalized weights together in a random direction, evaluate both presets, then use the difference to update the weights. Each update needs two score estimates regardless of the number of weights. Each estimate may require many games. Coarse sweeps make sense for categorical decisions such as factory queue recipes, where a small numeric perturbation can leave all orders identical. [Spall’s implementation guide](https://www.jhuapl.edu/spsa/PDF-SPSA/Spall_Implementation_of_the_Simultaneous.PDF) · [Stockfish tuning workflow](https://official-stockfish.github.io/docs/fishtest-wiki/Creating-my-first-test.html)
* **Learning the training settings.** The age at which the bot discounts a factory sighting is a policy parameter. The perturbation size and update step used to learn it are optimizer hyperparameters. Population Based Training periodically copies a successful learner and varies its training settings. This becomes worth testing if we have several learners whose training trajectories justify the extra games. [Population Based Training](https://arxiv.org/abs/1711.09846)

The seeded `replay_opponent` evaluator accepts a Tactical profile for each seat. For each turn it constructs the same fog-filtered published timeline as the live bot, applies the normal draft precheck and exact-state validation, and uses the simulator's scoreboard resolution. Both drafts are chosen against the same revision. Each match records rules and content fingerprints, seed, seat, parameter fingerprints, accepted-turn hashes, round stop reasons, scoreboard results, draft rejections and timing. The offline run omits live transport and archival writes. Its synthetic equal planning times do not affect the configured scoreboard, which has no time penalty.

The first search uses two passes over the number of grunts produced by Tactical's first combat factory before its tank loop. It checks the current center plus and minus six grunts, then plus and minus two around the winning center. Each candidate plays the frozen eight-grunt incumbent in both seats on the same training map. The sum of candidate-minus-incumbent scoreboard points at the two-round cap ranks candidates. The held-out maps receive only the best changed candidate, with the regular first-to-three victory condition and an outer six-round cap. This design uses one training seed block and two held-out blocks. The thresholds, seed budget and single-opponent pool are experimental choices. A longer run across specialists and recorded player plans is still needed to establish general strength.

```sh
taskset -c 0 nice -n 15 cargo build -j1 --release -p atemporal-server --example replay_opponent
taskset -c 0 nice -n 15 python3 scripts/tune-tactical.py --output tactical-first-pass.jsonl
```

The evaluator also accepts stand-alone profiles for manual comparisons. Pass `--self-play ROUND_CAP BOT_PARAMS_JSON OPPONENT_PARAMS_JSON` after the existing seed, style and seat arguments. A Tactical profile contains `opening_grunts`, `fresh_entry_ticks` and `rally_distance_band`. The latter two retain their defaults of 600 ticks and 8 route tiles in this search. This run leaves the release profile unchanged for independent review.

### First run on rules 11

The [match log](balance-data/tactical-self-play-rules11.jsonl) records 12 seeded games on one immutable evaluator binary. It used the code at `33e21cd` and one simulation thread. The games took 257.3 CPU seconds in total. All drafts passed validation.

On training seed 42, candidates with 2 and 14 opening grunts in iteration 1 and 6 and 10 in iteration 2 all scored 0–0 in both seats by the two-round cap. The incumbent's opening draft stayed identical at each seat across all four comparisons. Each challenger emitted a distinct opening draft at each seat. Both iterations kept eight grunts as the search center. The diagnostic holdout selected two grunts because it was the first of four equally scored challengers.

| Held-out seed | Challenger seat | Score, challenger first | Termination |
| --- | --- | --- | --- |
| 113 | 0 | 3–0 | Challenger win |
| 113 | 1 | 1–0 | Round cap |
| 907 | 0 | 1–3 | Incumbent win |
| 907 | 1 | 0–3 | Incumbent win |

The challenger went 1–2 in completed held-out matches and had a combined scoreboard margin of −1 including the capped game. The training screen supplied no ranking signal and the two held-out map blocks supplied no basis to replace the incumbent. Tactical 2.0 keeps its eight-grunt opening.

## Simple win

The game-type selector includes `simple_win`. It uses configurable scoreboard rules (three timeline wins by default), with no advancing history. Only completed structures count as buildings, including silos; construction ability is irrelevant. The first tick ending with any player lacking buildings ends the simulation, including in teams/FFA. Same-tick losses are resolved together. This cutoff overrides `stop_when_decided`, queued orders, missiles and minimum horizons. These multiplayer and completed-building details are implementation interpretations. Other game types retain their existing survival and cutoff rules.

## Private end-of-game feedback

Public coordinator game-end menus include Give feedback. A message is POSTed to `/api/games/<id>/feedback` and stored as `games/<id>/replays/<match-id>/feedback/<submission-id>.json` under the coordinator data root. Records include the exact match log path, game/match IDs, viewed revision, browser identity/name, receipt timestamp and message. The UI accepts up to 5,000 characters and keeps unsent text on failure. A per-message submission ID makes retries idempotent across coordinator restarts. The write is flushed before success is reported.

Feedback has no read/list endpoint and is not included in the public directory. It remains on disk with the archived game even after public viewing expires. Existing lobby password access and a known browser identity are required; these remain casual identity/access controls, not verified accounts. The creator can inspect the JSON files directly. There is no email delivery, public inbox or standalone-worker feedback route.

## Player timeline verification

The November 18 checks cover equal observations, selected ticks and drafts after a hidden-only state change, pending opponent orders, and reading an evicted revision after a later round publishes. The satellite regression observes a building between ordinary samples, retains its last sighting through an unseen removal, then clears the remembered building when recon shows the empty tile. The [browser comparison](browser-testing.md#player-visibility-and-bot-observations) checks the browser against an export of the same archive.

In a read-only export of a recorded rules-7 match, revision 4 reproduced its terminal hash and produced 15,708 player frames (ticks 0–15,707). Exporting player 0 and consuming the JSON lines took 4.7 seconds on the development host with two-core affinity. The exporter peaked at 53.9 MiB RSS. This is one archived workload. Peak resource use at 2,000 live entities remains unmeasured. The seeded evaluator also completed a first-to-three match between Tactical and Standard on seed 42.

Replaying revisions 2, 4 and 9 of that match through both Tactical policies gave six before-and-after drafts, one per seat and revision. In revision 4 the updated policy changed five squad destinations and reduced three silo plans from three target tiles to two. In revision 9 it redirected a 61-unit group from (6, 6) to (24, 2). The recorded match supplied identical observations for each comparison. Separately, Tactical playing seat 1 against Standard won 3–0 on seeds 42 and 211 with both policies. Those matchups establish no change in win rate.
