# Current work

- [x] Implement and verify attack-order paths, selection previews, factory queue movement types, settings and target corners.
- [x] Update delivery policy, merge completed path-preview changes into `dev`, and push.
- [x] Start a local server and share its reachable address.
- [x] Redesign artillery to distinguish it from tanks, review it in the browser, and push to `dev`.

Execute these tasks sequentially. Ready changes go to `dev`; updating `main` requires explicit user approval.

Local preview: http://192.168.1.138:8080/ (Tailscale: http://100.68.14.13:8080/). Server log: `/tmp/atemporal-dev-server.log`; PID file: `/tmp/atemporal-dev-server.pid`.
