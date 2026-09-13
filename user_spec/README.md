# User specification

Read these three documents for the user's requirements:

1. [Original prompt](core_prompt.md) — the full initial game concept and architecture-pass request, preserved unchanged.
2. [Gameplay and match rules](gameplay.md) — opening units, constructor mining, elimination, simultaneous turns, simulation stopping, timed mode, scoring, teams, and server launch.
3. [Orders and visual design](controls_and_visuals.md) — future-order replacement, persistent control groups, factory inheritance, presentation, the player guide, and lobby identity.

The thematic documents consolidate later user input and supersede conflicting parts of the original prompt. Repeated confirmations are combined. They contain user requirements, not automatic approval of the assistant's implementation proposals in `../docs/`.

## Specification maintenance and authority

The repository's top-level README should be a human-readable introduction and usage guide.

Only decisions actually made by the user are locked. Do not turn assistant defaults, data contracts, scope limits, or architecture choices into self-imposed requirements.

Keep future input in this folder by updating the appropriate thematic document. Prefer a few readable documents over one numbered file per interaction; introduce another document only for a substantial new topic. Preserve requirements and their intent, and distinguish unresolved interpretations from user decisions. Update affected architecture links and proposals as needed.

The user's latest storage instruction supersedes the earlier request for a new file for every interaction:

> Do a pass on the user_spec folder to consolidate. I don't need a ton of discrete files for every single interaction, a few big docs is much easier to read.

## Planning handoff

The user requested committing and pushing all planning documents when ready, and identifying any large remaining questions for them to answer. They reconfirmed committing and pushing once the player-guide, lobby-profile, and CLI-port planning update is settled. Remaining questions and recommendations belong in the architecture decision register until answered.

## Implementation and playtest expectations

The user requested review of the adversarial agent's findings and explicitly rejected its early-human-playtest recommendation: they only want to try playing once the game is ready according to the full current plan. Automated scenarios, agent-run browser checks and performance measurements can run during implementation. Do not introduce a reduced-scope human-playtest gate or require the user to play an unfinished slice.

Keep the main planning documents focused on the current plan, without references to review findings, reviewer identifiers or response history. Preserve any review record separately. Fold accepted planning changes into the current architecture without carrying response history into core docs, then commit and push the completed updates.

The user subsequently requested implementation of “Resolve product rules and establish contracts — coordinator” from `docs/implementation.md` in a new branch and worktree, with commits and pushes as substantial chunks are completed, reconciliation against current main, and integration into main after checks pass. This supersedes the planning-only execution note for that scope; it does not approve unrelated assistant product interpretations.

The user also requested a repeatable browser UI review harness and an interactive browser-control layer. Keep both usable by agents generally, rather than tying them to Codex. Agents should be able to iteratively inspect and improve the actual rendered game UI.


## Browser automation

The user requested an agent-neutral Playwright/MCP harness with useful screenshots, traces and failure artifacts. Verify the existing scaffold honestly and enable gameplay scenarios as real controls become available. Keep this work scoped to automation and browser-review documentation; coordinate package changes with the slice agent. Work in a new worktree and branch based on `main`, commit and push cohesive chunks, and integrate the completed work into `main` after local checks pass as the next agent in the merge queue. The latest instruction supersedes the earlier CI request: keep checks local, with no hosted jobs or automatic push triggers. See [browser testing](../docs/browser-testing.md) for the current workflow and explicit coverage limits.

## Complete browser implementation

The user requested the complete browser checklist on `agent/client` in `../atemporal-client`, cohesive commits and pushes, real-server Playwright input scenarios with screenshot inspection, and `scripts/check.sh`. Client ownership includes guide prose. The user subsequently authorized the necessary replay protocol/server change so historical results and authoritative skipped reasons can be inspected. Keep that change separate and ready to rebase onto another agent’s in-flight server work.

The final UI request uses a new branch/worktree, resolves the consolidated playtest requirements, and then completes any remaining “Complete browser experience” items and the “Full integration and handoff” checklist. Commit the playtest-fix batch before the remaining multiplayer, single-order and recovery browser work.

The final UI handoff must include manual review of full rendered game frames. Continue independently through fixes, browser scenarios and integration until the game is player-ready; the separately assigned native peripheral does not block UI work. Rebase the UI branch onto current main to include simulation fixes before final verification.

## Final UI and peripheral integration

The user requested resolving the playtest UI notes, completing the browser and integration checklists in a new worktree/branch, and manually inspecting full rendered game frames before handoff. The separately implemented input-only peripheral was non-blocking for UI work. The latest instruction is to rebase the UI implementation onto `agent/peripheral`, resolve conflicts, run tests, update the TODO list with actual remaining work, and push the integrated result to `main` when ready.
