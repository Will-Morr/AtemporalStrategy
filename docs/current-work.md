# Current work

- [x] Implement and verify attack-order paths, selection previews, factory queue movement types, settings and target corners.
- [x] Update delivery policy, merge completed path-preview changes into `dev`, and push.
- [x] Start a local server and share its reachable address.
- [x] Redesign artillery to distinguish it from tanks, review it in the browser, and push to `dev`.

Execute these tasks sequentially. Ready changes go to `dev`; updating `main` requires explicit user approval.

Local preview: http://192.168.1.138:8080/ (Tailscale: http://100.68.14.13:8080/). Server log: `/tmp/atemporal-dev-server.log`; PID file: `/tmp/atemporal-dev-server.pid`.

## Path and tread refinements

- [x] Inset and scale target corners; retain mining destinations and respect claims.
- [x] Preserve artillery's gun and replace its side supports with segmented treads.
- [x] Alternate axes for equally good tread steps in simulation and previews; keep tanks/artillery orthogonal.
- [x] Run movement/claim regressions and browser scenarios; inspect screenshots.
- [x] Merge and push to `dev`, and refresh the local preview.

Current rules-8 build is on port 8080 with fresh test data. The temporary server on 8081 has been stopped. Future ready updates replace the server on 8080, including during active testing; reset incompatible test games as needed.

## Dotted routes

- [x] Render route previews as dots, inspect the browser output, push to `dev`, and replace the test server on port 8080.

## DEBUG_TEST opening shortcut

- [x] Add an opt-in test-server flag, homescreen button and bot opening with human takeover at turn 2.
- [x] Verify the disabled flag, bot opening, handoff, refresh/restart and continued human play; inspect screenshots.
- [x] Push to `dev` and replace the local server on port 8080 with test mode enabled.
