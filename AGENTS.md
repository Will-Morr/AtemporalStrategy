# Project instructions

## Game-log backup before live-server cleanup

**Never delete game logs from the live server until a local copy has been confirmed.** Game logs are everything under the server's `STATE/games` (games, replays, results, feedback, visits). This applies to manual commands, scripts and the hourly job, even when the disk is full.

- Keep one local mirror, updated incrementally; do not create repeated full copies. Server code, release builds and executables need no backup and may be removed.
- Confirm each file's content against the mirror (checksum) before deleting it; keep anything that does not match. Stop and report if the sync or confirmation fails.
- `scripts/server-maintenance.py` implements this sequence and is what the hourly cron job runs; see `docs/hosting.md`.

- Read `user_spec/README.md` and the relevant user specifications before working.
- Consolidate user input into the appropriate thematic document under `user_spec/`; do not create a numbered file for each interaction. Preserve requirements and intent, combine repeated confirmations, and make superseding decisions clear. Keep `core_prompt.md` unchanged as the original reference. Add a new document only for a substantial new topic, and update the index and links as needed.
- Only decisions actually made by the user are locked. Treat assistant architecture, defaults, contracts, scope limits, and workflow choices as revisable proposals, even when drafted in imperative language.
- Later user instructions supersede earlier conflicting requirements. Do not impose an approval gate just because an assistant proposal says it is required.
- Keep `docs/` consistent with user decisions and label unresolved interpretations plainly.
- Main planning documents state the current plan without reviewer identifiers or response history; keep any historical review record separate.
- For browser UI changes, use the agent-neutral workflow in `docs/browser-testing.md`: run relevant real-browser scenarios, inspect screenshots/traces, and record actual verification limits. Do not treat DOM checks alone as visual review.
- Merge and push all ready changes to `dev`. Update `main` only when the user explicitly approves that update. This supersedes older integration instructions naming `main`.
- Keep the local test server on port 8080 current with ready changes. Restart/replace it even if the user is playing; these sessions are disposable tests. Reset incompatible test games as needed rather than preserving an old server on 8080 or moving the updated build to another port. No further confirmation is needed.
