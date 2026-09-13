# Provisional contract version 2

The [Rust records](../crates/contracts/src/types.rs) and [generated JSON Schema](../schemas/contracts-v2.json) implement the shared foundation for the [current architecture](contracts.md). They remain provisional until the real sim/server/browser slice passes Gate 2. This is not a frozen contract or a playable engine. Version 2 rejects version 1 records explicitly: digest identities and controller suppression records are incompatible, and no archive migration is provided.

Tagged unions use `kind` with snake_case names. Network envelopes carry numeric `schema_version: 2`; server replies also carry `server_instance_id`. Unknown versions/fields are rejected. Optional fields serialize as null. Wire integers use bounded u8/u16/u32 or `SafeInt` (0..9007199254740991). Group slots are 0–9. Ranges are inclusive unless explicitly exclusive. S[t] is before tick t; executing t produces S[t+1]. Schema validation does not replace exact-state ownership/capability/phase validation.

## Identities and checkpoints

CommandId is `{round, player, index}`. BirthCommandId adds `target_index`; EntityId adds `item_index` and `occurrence`. QueueItemId omits occurrence. The `identity` helpers construct these fixed-size records without hashes, recursive factory IDs or a persisted collision registry. Genesis reserves round zero and uses the initial roster slot. Blueprint sites retain the same identity on completion. Queue target indices refer to the full frozen sorted factory list, including unavailable factories. Item/target indices reject overflow; loop occurrence advances only at successful output insertion, which remains engine work.

`draft::resolve_local_references` validates sequential backward blueprint/queue references, unique local IDs, sorted unique factory targets and index bounds. Draft references distinguish persistent tuples from `{local_id,item_index}`. Blueprint and queue reference types are separate generic parameters. The helper never predicts births or replaces server metadata projection. Round zero is forbidden for committed draft resolution.

Canonical world JSON sorts keyed sets and validates finite resources/health, grids, occupancy and transition history. SHA-256 remains the state/content fingerprint, not an entity identity. Queue occurrence counters serialize as sorted records rather than JSON maps with object keys. Cooldowns, facing, targeting, birth timing, `goal_settled`, bounded `local_detour`, entity/group locks and survival transitions affect hashes. Flow-field grids, LRU presence and structure-cache versions are absent. Build/platform repeatability remains the limit of the f64 guarantee.

## Order locks and published commands

`locks::{blocks,install,canonicalize}` implement following intervals, older-round eligibility, overlapping windows, expiry and dominance. Keep preserves existing locks. DropWindow covers t < u <= min(t+W,max_tick-1); DropAll covers the remaining horizon. A short newer window does not extend its round threshold over an older longer window. Canonicalization preserves restrictions at the current tick as well as following ticks.

The sim must check ownership/capability and existing locks before calling install after a successful assignment. Missing/blocked assignments install nothing. Member locks affect that member; group-slot locks guard the saved-order write and whole delivery. Births inherit active group locks. Those application paths and conservative inactivity lookahead await the real engine; pure helper tests do not establish gameplay integration.

CommittedCommand and SimRequest contain no suppression sets. `GetCommands{revision,from_tick,to_tick}` / `Commands{revision,turns}` provide published command ranges for timeline inspection and local non-authoritative lock estimates. No server deletion-preview or per-entity/group removal-history endpoints remain. Pending opponents' unclosed-round inputs must never be exposed. Execution outcomes include `locked_by_later_round`.

## Execution, content and guide

SimRequest and WorkerMessage are owned Rust job/result values for an in-process dedicated thread. WorkerEnvelope is only a serialization fixture wrapper; there is no stdin/stdout framing or subprocess protocol. The server still needs a real adapter with bounded byte-accounted channels, cancellation, short exact-state job priority, stale-result rejection and archive recovery. Runner reserves only native peripheral work. Sim-local Clippy checks disallow HashMap/HashSet.

Content loading/normalization is shared. Optional `atemporal-content/guide` contains one generator; the tools crate enables it, while sim-only builds do not. Each server startup invokes `write_guide` using effective loaded content into a writable served directory before publishing its guide URL. Resume uses `load_archived_content` with the expected hash and the same generator. No bundled-guide selection/cache fallback remains. The scaffold preview implements this startup path; browser compilation generates only schema/types/assets. `fixtures/guide` is an authored validation export and is never served or copied into the build.

Config now includes `ore_matter_per_start`; example defaults are 24,000 matter and max_tick=20,000. At the configured dedicated miner rate of 2 matter/tick, that is an uninterrupted 12,000-tick depletion estimate. Map generation, lobby estimate display and measured depletion remain slice work. Costs, map bounds and all tuning are revisable.

## Existing reducers and remaining integration

Round scoring preserves the complete-vector-before-victory rule, configurable default target 5, survivor/team awards, zero stalemate awards, draw/tie policies and optional fastest-opponent timing adjustment. Timed adjudication checks completed constructor/factory capability in exact locked S[new_L], independently of active-building loss. StopAndArchive remains unfinished administrative metadata without another award. Durable exactly-once publication is server work.

Eight authored worlds and the golden comparator define acceptance inputs/expectations; they do not execute a simulation. The partial-group-lock fixture expects a skipped member and a retained group saved-order write. Quiet includes an inspectable final-state hash; other complete hashes await engine verification. Initial inactivity deadline is a state boundary: stall 3 with no activity stops at S[3]. Progress at tick t sets deadline t+1+stall_ticks; future potentially effective input still gets its opportunity.

Bulk sampled records are provisional full-state fixtures. Compact per-revision entity dictionaries, conservative u16/u32 selection and compact event/snapshot codecs remain Gate 2 integration work, alongside shared BFS fields, actual lock delivery/inheritance and arbitrary-tick reconstruction. Stabilize these using real payload and latency measurements; do not fan out dependent subsystem implementation against the current fixture-only transport.
