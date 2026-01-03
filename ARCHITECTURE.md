# Architecture

## Workspace overview
- services/traderd: main daemon
- services/traderctl: local CLI admin tool (unix socket)
- crates/venue_polymarket: Polymarket adapter (ws/rest/auth/signing)
- crates/execution: order manager + idempotency + retries
- crates/state: orderbook builder + feature extraction + snapshots
- crates/strategies: modular strategies emitting Intents
- crates/arbiter: locking + tiered selection + risk gating
- crates/risk: hard vetoes + caps + drawdown halts + kill switch
- crates/storage: sqlite event + trade logging
- crates/metrics: prometheus exporter + counters/histograms
- crates/sim: replay + shock injection + opponent heuristics

## Hot-path sequence (simplified)
1) venue_polymarket WS -> MarketEvent/UserEvent
2) state manager updates books/positions/features
3) StateSnapshot emitted
4) strategies propose intents
5) arbiter selects approved intents
6) execution sends orders, receives fills
7) storage logs + metrics increment

## Market ownership locks
- one active owner per market for ~250–1500ms lease
- owner can PlaceOrder
- others only Cancel/Flatten
- prevents self-crossing, inventory chaos, and intent wars

## Failure modes + mitigations
- WS disconnect -> mark snapshots stale -> risk veto all PlaceOrder
- partial fills -> execution tracks open exposure -> unwind policy
- API errors -> backoff + circuit breaker
- time drift -> monotonic clock + sanity checks

End.
