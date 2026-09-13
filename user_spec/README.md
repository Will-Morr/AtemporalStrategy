# User specification

Read these three documents for the user's requirements:

1. [Original prompt](core_prompt.md) — the full initial game concept and architecture-pass request, preserved unchanged.
2. [Gameplay and match rules](gameplay.md) — opening units, constructor mining, elimination, simultaneous turns, simulation stopping, timed mode, scoring, teams, and server launch.
3. [Orders and visual design](controls_and_visuals.md) — future-order replacement, persistent control groups, factory inheritance, presentation, the player guide, and lobby identity.

The thematic documents consolidate later user input and supersede conflicting parts of the original prompt. Repeated confirmations are combined. They contain user requirements, not automatic approval of the assistant's implementation proposals in `../docs/`.

## Specification maintenance and authority

Only decisions actually made by the user are locked. Do not turn assistant defaults, data contracts, scope limits, or architecture choices into self-imposed requirements.

Keep future input in this folder by updating the appropriate thematic document. Prefer a few readable documents over one numbered file per interaction; introduce another document only for a substantial new topic. Preserve requirements and their intent, and distinguish unresolved interpretations from user decisions. Update affected architecture links and proposals as needed.

The user's latest storage instruction supersedes the earlier request for a new file for every interaction:

> Do a pass on the user_spec folder to consolidate. I don't need a ton of discrete files for every single interaction, a few big docs is much easier to read.

## Planning handoff

The user requested committing and pushing all planning documents when ready, and identifying any large remaining questions for them to answer. They reconfirmed committing and pushing once the player-guide, lobby-profile, and CLI-port planning update is settled. Remaining questions and recommendations belong in the architecture decision register until answered.
