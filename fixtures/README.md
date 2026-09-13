# Shared integration fixtures

`manifest.json` lists concrete boundary values and their generated schema types. `npm test --prefix client` validates every listed value with JSON Schema in JavaScript and checks rejection of malformed versions, group slots, unknown fields, unsafe integers, and numeric overflows. Rust contract/content tests separately validate parsing and semantics.

Golden-world fixtures are coordinator-defined acceptance inputs and expected actions/outcomes. They are not an alternative game engine or evidence that the simulation subsystem is implemented. The simulation handoff must run each request through the real engine, compare every specified state/action/result, and then add full replay/checkpoint/thread-equivalence hashes. Omitted expected entity fields mean “not asserted”; `present: false` means the ID must be absent, never rebound to a replacement.
