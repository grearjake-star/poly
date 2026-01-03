# spec.md — Polymarket Multi-Strategy Bot (Rust) “Bot Bible”

## 1) Mission
Build a safe, execution-first, multi-strategy Polymarket CLOB trading system that:
1) Captures structural edge (arb/event structure) when it exists,
2) Provides steady churn via reward-aware market making when arb is scarce,
3) Adds adaptive decisioning (ML -> PPO later) behind strict risk + veto gates,
4) Treats fill realism, latency, and adversarial conditions as first-class.

Non-negotiables:
- No manipulation (spoofing/wash/quote stuffing).
- Full audit trail, robust key management, instant kill switch / flatten.

---

## 2) Strategy stack (priority order)
### Tier 0 (highest): Arb family
A1) Box/complement arb:
- Trigger: ask_yes + ask_no < 1 - buffer
- Hard part: two-leg execution + partial fills

A2) Related-market / multi-outcome arb:
- Mispriced probability mass across outcomes / baskets
- Often less crowded than box arb

A3) NegRisk / event structure arb:
- Exploit event-level structural transforms (shadow-first until accounting proven)

### Tier 1: Reward-aware market making (MM)
- Quote both sides around internal fair probability
- Inventory-aware skew
- Toxicity/crowding/spread-compression veto filters
- Optional reward optimization if applicable

### Tier 2 (lowest): Directional / fused-state
- Fused state: Binance microstructure + Polymarket orderbook + internal inventory/risk
- Start with supervised/bandit models:
  - fill probability / adverse selection
  - fair probability calibration
- Later PPO produces *intents* only (not direct order control), gated by Arb/MM/risk vetoes

---

## 3) System architecture (modules)
### 3.1 Execution kernel (boring, correct, fast)
- venue adapters (Polymarket, Binance)
- order manager (place/cancel/replace, partial fills, idempotency)
- portfolio/positions
- risk governor + kill switch + flatten
- storage (SQLite first)
- telemetry + metrics (Prometheus)

### 3.2 Strategy layer (modular)
Strategies output **Intents**:
- PlaceOrder / Cancel / Flatten / NoOp
Each intent includes EV/confidence/risk_cost/tags.

### 3.3 Arbiter (hierarchical gating)
- enforces priority tiers
- enforces per-market ownership locks (prevents strategies fighting)
- applies hard risk veto
- selects which intents execute

### 3.4 Metrics registry (truth serum)
- execution quality: latency, fill rate, partial fill rate, slippage proxy
- strategy attribution: PnL by strategy, hit rate, drawdown contribution
- opponent heuristics: crowding/toxicity/spread compression
- system health: WS disconnects, error rate

### 3.5 Plugins (optional signals)
- sentiment/news (rate-limited, event-triggered)
- whale/on-chain regime flags
- esports signals (ONLY for esports markets)
- chain health / congestion monitor (risk tightening)

Plugins are disabled by default. They must earn their keep via A/B testing.

### 3.6 Sim/replay (first-class)
- event recorder
- deterministic replay harness
- adversarial shock injection (latency/slippage/book thinning)
- opponent heuristics evaluation in replay

---

## 4) Data flow (hot path)
WS streams (market + user) + external streams
  -> MarketData Bus
  -> OrderBook Builder + Feature Extractor
  -> StateSnapshot per market tick
  -> Strategies -> Intents
  -> Arbiter (risk + locks + tier)
  -> Execution (orders)
  -> Fills/updates -> Portfolio + Metrics + Storage
  -> Repeat

---

## 5) Key design rules (to prevent self-destruction)
1) Market ownership: one “active” strategy owns a market at a time.
2) Arb > EventArb > MM > Directional priority.
3) Any strategy may propose Cancel/Flatten; only the owner can propose PlaceOrder (unless emergency).
4) Risk veto is absolute.
5) Directional RL/ML cannot override Arb/MM locks; it only proposes.
6) No strategy touches private keys directly; signing happens in venue adapter only.

---

## 6) “Opponent heuristics” (must implement early)
These are lightweight detectors that often beat MAS in practice:
- Crowding detector: edge half-life, depth drop, partial fill spikes
- Spread compression detector: min-tick spreads + low fills (bad queue position)
- Toxicity detector: fills cluster before adverse moves; imbalance spikes

Used to:
- widen arb buffers
- pull MM quotes
- gate directional actions

---

## 7) Adversarial randomization (sim/replay)
Inject realism:
- latency + jitter (signal->order, cancel/replace, fill notifications)
- slippage curve + partial fills
- book thinning shocks during “hot moments”
- regime flips (high vol / low liquidity windows)

Acceptance criterion:
- strategies produce stable performance distributions across random seeds
- edge disappears under crowding unless buffer expands (as it should)

---

## 8) Storage (SQLite first)
We store:
- raw events (append-only)
- normalized orders/fills
- per-strategy attribution snapshots
- health metrics snapshots

Design goal:
- append-only, crash-safe
- easy upgrade to Postgres later without changing crate interfaces

---

## 9) Observability + control plane
### Prometheus exporter
- `/metrics` served locally (bind to 127.0.0.1 by default)
- scrape via local Prometheus or SSH tunnel

### Local-only admin control channel (Unix socket)
- JSON-lines commands (pause/resume strategy, set caps, flatten market)
- NO public HTTP admin surface

---

## 10) Build milestones (with acceptance tests)
M1 Kernel boots:
- daemon runs, /metrics up, unix socket accepts commands, sqlite logs events

M2 Polymarket ingestion + order manager:
- can subscribe market/user WS
- can place/cancel in dry-run
- reconciles fills without position drift

M3 State snapshots + replay harness:
- recorded WS replay drives deterministic snapshots
- feature vector stable + validated

M4 Box arb shadow:
- emits intents only
- logs edge stats + crowding score

M5 Live micro box arb:
- tiny caps
- strict kill switch
- two-leg discipline + unwind

M6 MM shadow -> live:
- toxicity filter active
- inventory bounded

M7 Event arb/NegRisk shadow-first:
- accounting verified before any live trades

M8 ML inference -> PPO bolt-on:
- shadow mode A/B shows improvement
- gated live micro allocation only

---

## 11) Go-live cadence (VPS)
1) Shadow 48–72h (no orders)
2) Paper/live-sim 1–2 weeks (intents only)
3) Live micro box arb (caps + kill switch)
4) Enable MM in vetted markets
5) Enable event arb (shadow-first)
6) Enable ML/directional (shadow -> gated live -> scale)

---

## 12) Non-functional requirements
- P99 internal decision latency < 5ms on VPS
- graceful degradation on WS disconnects
- never place orders when state is stale
- every order attributable to (strategy, snapshot_id, rationale)
- deterministic replay from stored events
