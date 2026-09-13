# Shared integration fixtures

`manifest.json` lists concrete boundary values and their generated schema types. `npm test --prefix client` validates every listed value with JSON Schema in JavaScript and checks rejection of malformed versions, group slots, unknown fields, unsafe integers, and numeric overflows. Rust contract/content tests separately validate parsing and semantics.

Golden-world fixtures are coordinator-defined acceptance inputs and expected actions/outcomes. They are not an alternative game engine or evidence that the simulation subsystem is implemented. The simulation handoff must run each request through the real engine, compare every specified state/action/result, and then add full replay/checkpoint/thread-equivalence hashes. Omitted expected entity fields mean “not asserted”; `present: false` means the ID must be absent, never rebound to a replacement.


| Fixture | Required result |
| --- | --- |
| quiet | S[3] inactive stalemate, zero economy, exact final hash |
| mining-half | t=0 orders visible in S[1]; miner 8 + constructor 4 matter by S[4] |
| future-input | Scheduled tick 5 mining defers the S[3] cutoff; six matter by S[8] |
| mutual-elimination | Both turrets die on tick 0; remaining constructors persist; draw only at S[4] |
| factory-recovery | A funded site completes on tick 0, restores its owner, and yields a non-scoring stalemate |
| dormant-cause | Absent causal target is skipped; existing units are untouched |
| group-birth | Existing member keeps its individual override; actual birth inherits the saved group order |
| partial-group-lock | One member is locked while the other delivery and saved group order remain |

Call `atemporal_contracts::golden::verify` with real engine snapshots, final outcome, and command outcomes. It checks every declared expectation and fails on missing samples. For hand-seeded checkpoints, `rng_state: "fixture:no_rng:v2"` means no random choices are needed in these explicit terrain scenarios; the simulation handoff should provide a compatible fixture initializer. `sim_build: "tiny-world-contract-v2"` is a fixture identifier, not a production binary fingerprint. Initial paid projects and statuses are seeded specimens, not claims that they arose from the free opening roster. Production fields on a site become active only at completion.

`crates/tools/src/fixtures.rs` is the authored source of golden-world data. It computes causal IDs and input fingerprints with shared code and never runs a simulation. Regeneration preserves separate protocol specimens and sorts the manifest. `command-variants.json` covers all command envelope shapes, including illustrative dormant factory/blueprint IDs; it is a transport parsing fixture, not a valid live opening commit. Other protocol samples are independent boundary records, not a single server session.

Inactivity uses the [tick conventions](../docs/contracts-v2.md#existing-reducers-and-remaining-integration). The pure quiet final hash can be checked by inspection; other full final hashes await the real engine. Sharing expected outcome data with a test does not establish engine correctness, which is why the simulation checklist remains open.
