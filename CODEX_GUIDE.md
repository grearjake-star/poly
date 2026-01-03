# Codex Guide: How to implement this bot without making a mess

## Golden rules
1) Implement kernel first (venue -> state -> arbiter -> execution -> storage -> metrics).
2) Strategies only emit Intents; they do not place orders directly.
3) Arbiter owns priority + locking logic.
4) Risk veto is absolute.
5) Every PR must include tests or a replay artifact update.

## Work plan for Codex (ordered tasks)
Task 1: Workspace scaffolding + CI + clippy + rustfmt
Task 2: domain types + config loader + telemetry init
Task 3: metrics exporter + sqlite storage + event logging
Task 4: admin_ipc server + traderctl CLI
Task 5: venue_polymarket ingest skeleton (market/user WS)
Task 6: state manager: orderbook top-of-book + stale detection
Task 7: arbiter + risk governor wiring
Task 8: execution stub (dry-run), then real place/cancel
Task 9: boxarb strategy (shadow mode)
Task 10: replay harness + opponent heuristics v0

## Definition of Done (per milestone)
- builds on clean machine
- `cargo test` passes
- `traderd` runs in shadow mode and logs snapshots
- `/metrics` shows heartbeat + WS health
- `traderctl status` returns live state

End.
