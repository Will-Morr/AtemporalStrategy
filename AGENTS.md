# Project instructions

- Read `user_spec/README.md` and the relevant user specifications before working.
- Consolidate user input into the appropriate thematic document under `user_spec/`; do not create a numbered file for each interaction. Preserve requirements and intent, combine repeated confirmations, and make superseding decisions clear. Keep `core_prompt.md` unchanged as the original reference. Add a new document only for a substantial new topic, and update the index and links as needed.
- Only decisions actually made by the user are locked. Treat assistant architecture, defaults, contracts, scope limits, and workflow choices as revisable proposals, even when drafted in imperative language.
- Later user instructions supersede earlier conflicting requirements. Do not impose an approval gate just because an assistant proposal says it is required.
- Keep `docs/` consistent with user decisions and label unresolved interpretations plainly.
- Main planning documents state the current plan without reviewer identifiers or response history; keep any historical review record separate.
- For browser UI changes, use the agent-neutral workflow in `docs/browser-testing.md`: run relevant real-browser scenarios, inspect screenshots/traces, and record actual verification limits. Do not treat DOM checks alone as visual review.
