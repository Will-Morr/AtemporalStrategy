# Implemented contract version 1

This is the coordinator handoff. [Rust records](../crates/contracts/src/types.rs) and the [generated JSON Schema](../schemas/contracts-v1.json) define the concrete wire format. The earlier [contracts](contracts.md) explain semantics; when their pseudocode differs, use these executable v1 records. Version 1 is the integration baseline, not user approval of every design choice. A incompatible wire/identity change requires a version bump, regenerated artifacts, fixtures, and a documented migration or explicit rejection of old archives.

All tagged unions use `kind` with snake_case names. Every browser/worker envelope has numeric `schema_version: 1`; unsupported versions and unknown fields are rejected. `SimRequest`, content, setup, and world records also carry versions. A server envelope always carries `server_instance_id`, including range replies. `Hello.slot_token` restores an existing private identity; public lobby profiles never contain tokens. Optional record fields serialize as explicit null; receivers accept missing optional fields. Ranges are inclusive unless the field says `exclusive`. `S[t]` is before tick t, and executing tick t produces `S[t+1]`.

`SafeInt` constrains seeds and persisted durations to 0..9007199254740991. Other wire integers are u8/u16/u32. Group slots are 0..9. Structural schema checks do not replace ownership, capabilities, phase, bounds, content, or score validation. The server must validate those before accepting a draft. Floating-point state is finite f64; canonical world/content validators reject invalid values before hashing. No cross-platform floating-point promise is added.

## Causal identity and canonical encoding

Use `identity` functions; never allocate births with a global counter. Command IDs are `r{round}:p{player}:c{index}` with decimal integers without leading zeroes. Player IDs start at 0; scored rounds start at 1. Production and blueprint indices are zero-based command-array indices and are never renumbered by suppression. Round precedence rotates by `round % player_count` and is persisted separately in `SimRequest.precedence` and replication records.

A causal ID is `<namespace>:<64 lowercase SHA-256 hex digits>`. Hash the UTF-8 compact JSON array `["atemporal",1,namespace,parts]`:

| Namespace | Parts |
| --- | --- |
| entity | `["genesis", player, initial_roster_slot]` |
| blueprint | `[command_id, tile_index]` |
| entity | `["structure", blueprint_id]` |
| queue | `[factory_id, command_id, item_index]` |
| entity | `["production", factory_id, queue_item_id, occurrence]` |

Occurrence begins at 0 and increments at successful spawn, including loop repeats. Tick, content type, display profile, and queue position are absent from birth identity. Checkpoint `deterministic_identity_state` stores ID → preimage for collision checks; regenerate the registry from the replay base rather than retaining abandoned suffix entries. IDs remain bounded even when factories eventually produce constructors that build factories.

Canonical JSON sorts object keys lexicographically, uses compact serde_json number encoding, and normalizes negative floating zero to positive floating zero. World hashing additionally sorts players by ID, entities/blueprints by ID, groups by owner/slot, group member sets lexically, and elimination reasons in no-active-building/no-build-ability order. Reject duplicate keyed records and duplicate occupancy. Preserve terrain/ore row-major order, queue order, and command item/tile order. Derived occupancy and path caches do not appear in state. RNG state is an opaque simulation-owned checkpoint string; its encoding/algorithm must be pinned in `sim_build` before real generated-world fixtures are accepted. Hashes are exact only with the pinned implementation/platform. Profiles, score, wall-clock timing, and cosmetic playback lifetimes are outside `WorldState`.

## Scoring implementation and remaining Q4 interpretations

`scoring::resolve_round` is the shared controller reducer. It awards every resolved round (passes use exactly the same call), applies the entire delta vector before victory evaluation, and accepts only the next round after the last durable score. Retrying that last round returns the original result. Archive-level request/payload identity and crash-atomic persistence remain server work; the reducer alone does not provide durability.

Confirmed behavior: configurable target default 5; zero stalemate awards; win awards by final survivors; team totals count surviving members; optional lead-N compares the strongest rival; draw scoring and threshold ties are configurable; default tied leaders continue. Temporary elimination never affects endpoint scoring.

Implemented, revisable Q4 interpretations:

- Each credited FFA participant earns one raw point. Draw/all_players similarly credits one per original participant, summed by team; survivor lists remain empty. Default draw policy is none.
- Fixed target uses `>=`, with a uniquely highest side required by default. Shared victory returns all tied highest qualifying sides. Lead requires its positive margin even with shared victory selected.
- Optional `fastest_opponent_ratio` credits `min(1, max(fastest opposing player's total_ms,1000) / max(own total_ms,1000))`. Teams sum individual credits; a teammate is never a time-penalty opponent. Victory uses adjusted totals when enabled. All players' displayed ratios compare against the overall fastest participant, with the same floor.
- Exact f64 comparisons are used for adjusted-score ties. The default penalty is none. Timing formula, fractional tie tolerance, default optional lead margin, and timed-mode closure remain revisable; no additional product approval is required to proceed.

`RoundScore.round` identifies an award independently of simulation revision. The server pins all policies with the match and restores the last complete score record. It must never call the next-round reducer merely because a worker was retried. Timed revisions use `score: null`; timed closure remains server work.

## Subsystem boundaries

The workspace reserves sim/server/runner crates without claiming an implemented engine or transport. `crates/content` is the shared coordinator-owned loader/exporter: simulation should consume it and extend validation through contract changes rather than create a second type/stat schema. Server owns routing, runtime guide selection calls, durable scores, and real framed process IO. Worker framing is a four-byte unsigned big-endian byte length followed by UTF-8 JSON, maximum 16 MiB per frame; reject a larger length before allocation, and send logs only to stderr. The forthcoming worker implementation must split batches within this limit.

The client owns the full play experience and guide prose/layout. Bootstrap config is the pinned `MatchConfig`; live setup team availability is in `LobbyState`. `Setup.match_defaults.multiplayer` selects FFA or team mode; team assignments in examples are defaults only and must be replaced from accepted lobby choices before Start pins the match. `PreviewFutureOrders.preceding_commands` permits previewing membership edits earlier in the same draft; only the server resolves authoritative suppression records.
