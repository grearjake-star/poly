# docs/DATA_SCHEMA.md — Data Schema & Attribution (“Power Doc”)

This document defines the **minimum robust schema** to:
- reproduce bot behavior (deterministic-ish replay),
- attribute PnL by strategy and by decision,
- measure execution quality (fills, partials, latency),
- support safe ops (audit trail, incidents, kill switch),
- upgrade from **SQLite → Postgres** with minimal interface changes.

Design goals:
1) **Append-only where possible** (audit trail + crash safety)
2) **Normalized joins** (strategy → intent → order → fill → PnL)
3) **Replay-friendly** (store raw events + derived snapshots)
4) **Low write amplification** (avoid massive per-tick writes unless enabled)

---

## 0) Key identifiers & linking model

### Required IDs
- `run_id` (TEXT/UUID): unique per daemon start
- `snapshot_id` (INTEGER/UUID): unique per emitted `StateSnapshot`
- `intent_id` (INTEGER/UUID): unique per strategy intent proposal
- `approved_id` (INTEGER/UUID): unique per arbiter-approved intent
- `order_id` (TEXT): venue order id (string-safe)
- `fill_id` (TEXT): venue fill id (string-safe)
- `market_id` (INTEGER/TEXT): stable market identifier

### Golden link chain (attribution)
`strategy_intents.intent_id`
→ `arbiter_approvals.approved_id`
→ `orders.order_id` (and/or `orders.client_order_id`)
→ `fills.fill_id`
→ `pnl_ledger` rows

Everything should be traceable back to **the snapshot the bot saw** and the **strategy rationale**.

---

## 1) SQLite schema (initial)

> The canonical schema lives in SQLx migrations under `crates/storage/migrations`. `traderd` applies them automatically on startup. For manual runs (local dev or CI), use `sqlx migrate run --source crates/storage/migrations --database-url sqlite://bot.db` after installing `sqlx-cli` with `cargo install sqlx-cli --no-default-features --features sqlite`.

### 1.1 Schema versioning

CREATE TABLE IF NOT EXISTS schema_meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

INSERT OR IGNORE INTO schema_meta (key, value) VALUES ('schema_version', '1');

### 1.2 Runs (one row per daemon start)


CREATE TABLE IF NOT EXISTS runs (
  run_id TEXT PRIMARY KEY,
  started_at_ms INTEGER NOT NULL,
  git_sha TEXT,
  config_hash TEXT,
  host TEXT,
  notes TEXT
);


### 1.3 Raw event log (append-only; replay truth)
Store venue WS payloads and important internal events. This is your “black box flight recorder.”

CREATE TABLE IF NOT EXISTS raw_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id TEXT NOT NULL,
  ts_ms INTEGER NOT NULL,
  source TEXT NOT NULL,          -- 'polymarket_market_ws' | 'polymarket_user_ws' | 'binance_ws' | 'plugin' | 'internal'
  topic TEXT NOT NULL,           -- e.g. 'l2_update', 'trade', 'order_update', 'fill', 'heartbeat'
  market_id INTEGER,             -- nullable for non-market events
  payload_json TEXT NOT NULL,    -- original JSON string
  FOREIGN KEY(run_id) REFERENCES runs(run_id)
);

CREATE INDEX IF NOT EXISTS idx_raw_events_run_ts ON raw_events(run_id, ts_ms);
CREATE INDEX IF NOT EXISTS idx_raw_events_market_ts ON raw_events(market_id, ts_ms);
CREATE INDEX IF NOT EXISTS idx_raw_events_source_topic_ts ON raw_events(source, topic, ts_ms);


Retention guidance:

* Keep `raw_events` for 7–30 days locally (compress + archive off-box if needed).
* Consider feature flags to disable full L2 logging unless debugging.

### 1.4 Derived snapshots (`StateSnapshot`)

You do **not** need to store every tick by default. Store:

* snapshots only when strategies are evaluated, OR
* periodic snapshots (e.g., 1s), OR
* snapshots only when an intent is emitted.

Recommended: store snapshots when you run strategies (best replay value per byte).


CREATE TABLE IF NOT EXISTS snapshots (
  snapshot_id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id TEXT NOT NULL,
  ts_ms INTEGER NOT NULL,
  market_id INTEGER NOT NULL,

  -- Orderbook top-of-book
  best_bid_px REAL,
  best_bid_qty REAL,
  best_ask_px REAL,
  best_ask_qty REAL,
  spread REAL,

  -- Position & risk views
  yes_qty REAL NOT NULL,
  no_qty REAL NOT NULL,
  net_exposure_usd REAL NOT NULL,
  can_trade INTEGER NOT NULL,            -- 0/1
  drawdown_halt INTEGER NOT NULL,        -- 0/1

  -- Opponent heuristics
  crowding_score REAL NOT NULL,
  toxicity_score REAL NOT NULL,
  spread_compression REAL NOT NULL,

  -- Feature vector storage (JSON array)
  feature_schema_version INTEGER NOT NULL,
  features_json TEXT NOT NULL,

  FOREIGN KEY(run_id) REFERENCES runs(run_id)
);

CREATE INDEX IF NOT EXISTS idx_snapshots_run_market_ts ON snapshots(run_id, market_id, ts_ms);


### 1.5 Strategy intents (all proposals; append-only)

Every strategy proposal is logged (including rejected ones).

CREATE TABLE IF NOT EXISTS strategy_intents (
  intent_id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id TEXT NOT NULL,
  ts_ms INTEGER NOT NULL,
  snapshot_id INTEGER NOT NULL,

  strategy TEXT NOT NULL,          -- 'boxarb' | 'mm' | 'eventarb' | 'directional'
  market_id INTEGER NOT NULL,
  intent_kind TEXT NOT NULL,       -- PlaceOrder | CancelOrder | CancelAll | FlattenMarket | NoOp
  side TEXT,                       -- BuyYes | BuyNo | SellYes | SellNo (nullable)
  price REAL,
  size REAL,
  urgency TEXT NOT NULL,           -- Maker | Neutral | Taker
  ttl_ms INTEGER NOT NULL,

  expected_value REAL NOT NULL,
  confidence REAL NOT NULL,
  risk_cost REAL NOT NULL,
  tags_json TEXT NOT NULL,         -- JSON array

  rationale_json TEXT,             -- optional: structured explanation (small)

  FOREIGN KEY(run_id) REFERENCES runs(run_id),
  FOREIGN KEY(snapshot_id) REFERENCES snapshots(snapshot_id)
);

CREATE INDEX IF NOT EXISTS idx_intents_run_ts ON strategy_intents(run_id, ts_ms);
CREATE INDEX IF NOT EXISTS idx_intents_market_ts ON strategy_intents(market_id, ts_ms);
CREATE INDEX IF NOT EXISTS idx_intents_strategy_ts ON strategy_intents(strategy, ts_ms);


### 1.6 Arbiter approvals (which intents got through)

`
CREATE TABLE IF NOT EXISTS arbiter_approvals (
  approved_id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id TEXT NOT NULL,
  ts_ms INTEGER NOT NULL,
  intent_id INTEGER NOT NULL,

  approved INTEGER NOT NULL,          -- 0/1
  reason TEXT,                        -- 'ok' | 'risk_veto' | 'market_locked' | 'lower_priority' | 'invalid' etc.
  owner_strategy TEXT,                -- market owner at decision time (nullable)

  FOREIGN KEY(run_id) REFERENCES runs(run_id),
  FOREIGN KEY(intent_id) REFERENCES strategy_intents(intent_id)
);

CREATE INDEX IF NOT EXISTS idx_approvals_intent ON arbiter_approvals(intent_id);
CREATE INDEX IF NOT EXISTS idx_approvals_run_ts ON arbiter_approvals(run_id, ts_ms);


### 1.7 Orders (venue-facing submissions)

Orders represent what we actually submitted (or attempted).

Notes:

* Use `client_order_id` for idempotency.
* Store status transitions and timestamps for latency stats.


CREATE TABLE IF NOT EXISTS orders (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id TEXT NOT NULL,
  ts_submitted_ms INTEGER NOT NULL,
  approved_id INTEGER,                 -- nullable if manual/forced
  intent_id INTEGER,                   -- redundant but convenient

  strategy TEXT NOT NULL,
  market_id INTEGER NOT NULL,

  venue TEXT NOT NULL,                 -- 'polymarket'
  order_id TEXT,                       -- assigned by venue (nullable until ack)
  client_order_id TEXT NOT NULL,       -- our idempotency key
  status TEXT NOT NULL,                -- Submitted | Acked | Open | PartiallyFilled | Filled | Canceled | Rejected | Failed
  side TEXT NOT NULL,
  limit_price REAL NOT NULL,
  qty REAL NOT NULL,

  -- Timing metrics
  ts_acked_ms INTEGER,
  ts_final_ms INTEGER,
  submit_latency_ms INTEGER,           -- computed when acked (acked - submitted)
  notes TEXT,

  FOREIGN KEY(run_id) REFERENCES runs(run_id),
  FOREIGN KEY(approved_id) REFERENCES arbiter_approvals(approved_id),
  FOREIGN KEY(intent_id) REFERENCES strategy_intents(intent_id)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_orders_client_oid ON orders(client_order_id);
CREATE INDEX IF NOT EXISTS idx_orders_order_id ON orders(order_id);
CREATE INDEX IF NOT EXISTS idx_orders_market_ts ON orders(market_id, ts_submitted_ms);
CREATE INDEX IF NOT EXISTS idx_orders_strategy_ts ON orders(strategy, ts_submitted_ms);


### 1.8 Fills (trade executions)


CREATE TABLE IF NOT EXISTS fills (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id TEXT NOT NULL,
  ts_ms INTEGER NOT NULL,

  venue TEXT NOT NULL,
  fill_id TEXT,                     -- venue fill id if present
  order_id TEXT,                    -- venue order id
  client_order_id TEXT,             -- join key if order_id is missing or unstable
  market_id INTEGER NOT NULL,
  strategy TEXT NOT NULL,

  side TEXT NOT NULL,               -- BuyYes | BuyNo | SellYes | SellNo
  price REAL NOT NULL,
  qty REAL NOT NULL,

  fee_usd REAL DEFAULT 0.0,
  liquidity TEXT,                   -- maker/taker if available
  raw_json TEXT,                    -- optional, small

  FOREIGN KEY(run_id) REFERENCES runs(run_id)
);

CREATE INDEX IF NOT EXISTS idx_fills_order_id ON fills(order_id);
CREATE INDEX IF NOT EXISTS idx_fills_client_oid ON fills(client_order_id);
CREATE INDEX IF NOT EXISTS idx_fills_market_ts ON fills(market_id, ts_ms);
CREATE INDEX IF NOT EXISTS idx_fills_strategy_ts ON fills(strategy, ts_ms);


### 1.9 PnL ledger (append-only accounting)

We track:

* **realized PnL** from executed trades,
* **MTM/unrealized PnL** snapshots for equity curve,
* **fees** and other adjustments.


CREATE TABLE IF NOT EXISTS pnl_ledger (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id TEXT NOT NULL,
  ts_ms INTEGER NOT NULL,
  market_id INTEGER NOT NULL,
  strategy TEXT,                       -- nullable for portfolio-level rows

  kind TEXT NOT NULL,                  -- 'realized' | 'mtm' | 'fee' | 'funding' | 'adjustment'
  ref TEXT,                            -- fill_id/order_id/snapshot_id/other
  pnl_usd REAL NOT NULL,
  notes TEXT,

  FOREIGN KEY(run_id) REFERENCES runs(run_id)
);

CREATE INDEX IF NOT EXISTS idx_pnl_run_ts ON pnl_ledger(run_id, ts_ms);
CREATE INDEX IF NOT EXISTS idx_pnl_market_ts ON pnl_ledger(market_id, ts_ms);
CREATE INDEX IF NOT EXISTS idx_pnl_strategy_ts ON pnl_ledger(strategy, ts_ms);


### 1.10 Portfolio snapshots (optional but useful)

Store aggregated portfolio view periodically (e.g., every 5–30 seconds).


CREATE TABLE IF NOT EXISTS portfolio_snapshots (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id TEXT NOT NULL,
  ts_ms INTEGER NOT NULL,

  equity_usd REAL NOT NULL,
  realized_pnl_usd REAL NOT NULL,
  unrealized_pnl_usd REAL NOT NULL,
  gross_exposure_usd REAL NOT NULL,
  net_exposure_usd REAL NOT NULL,

  drawdown_usd REAL NOT NULL,
  drawdown_pct REAL NOT NULL,
  open_orders_count INTEGER NOT NULL,

  FOREIGN KEY(run_id) REFERENCES runs(run_id)
);

CREATE INDEX IF NOT EXISTS idx_portfolio_run_ts ON portfolio_snapshots(run_id, ts_ms);


### 1.11 Plugin signals (external features)

CREATE TABLE IF NOT EXISTS plugin_signals (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id TEXT NOT NULL,
  ts_ms INTEGER NOT NULL,
  plugin TEXT NOT NULL,            -- 'sentiment' | 'chain_health' | 'whale' | 'esports'
  scope TEXT NOT NULL,             -- 'crypto15m' | 'esports' | 'all'
  market_id INTEGER,               -- nullable if global
  payload_json TEXT NOT NULL,
  FOREIGN KEY(run_id) REFERENCES runs(run_id)
);

CREATE INDEX IF NOT EXISTS idx_plugin_run_ts ON plugin_signals(run_id, ts_ms);
CREATE INDEX IF NOT EXISTS idx_plugin_market_ts ON plugin_signals(market_id, ts_ms);


### 1.12 Health + incidents (ops)
CREATE TABLE IF NOT EXISTS incidents (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id TEXT NOT NULL,
  ts_ms INTEGER NOT NULL,
  severity TEXT NOT NULL,           -- INFO | WARN | ERROR | CRITICAL
  kind TEXT NOT NULL,               -- WS_DISCONNECT | RISK_HALT | EXEC_ERROR | STALE_STATE | etc
  message TEXT NOT NULL,
  payload_json TEXT,
  FOREIGN KEY(run_id) REFERENCES runs(run_id)
);

CREATE INDEX IF NOT EXISTS idx_incidents_run_ts ON incidents(run_id, ts_ms);


---

## 2) Feature schema versioning

Feature vectors evolve. You must version them to preserve replay meaning.

Rules:

* Every snapshot stores `feature_schema_version`.
* `state/` crate exposes a `FEATURE_SCHEMA_VERSION: i32`.
* Any change to feature definitions increments the version.

Recommended helper table:

CREATE TABLE IF NOT EXISTS feature_schemas (
  feature_schema_version INTEGER PRIMARY KEY,
  created_at_ms INTEGER NOT NULL,
  description TEXT NOT NULL,
  features_json TEXT NOT NULL        -- JSON array of feature names in order
);


---

## 3) PnL attribution: how we compute it

### 3.1 Realized PnL (fills)

For each fill, compute cashflow:

* Buy token: cashflow = `- price * qty - fee`
* Sell token: cashflow = `+ price * qty - fee`

Realized PnL is recognized when you **round-trip** inventory, typically via FIFO lots:

* Maintain lots per (market_id, token_side).
* Selling matches against lots to compute realized PnL.

Write `pnl_ledger(kind='realized', ref=fill_id)` with `strategy` populated.

### 3.2 MTM / Unrealized PnL

Periodic mark based on **conservative prices**:

* long YES: mark at best_bid_yes
* long NO:  mark at best_bid_no
* if shorts exist, mark at buy-to-cover side (best_ask)

Store MTM contributions as `pnl_ledger(kind='mtm', ref=snapshot_id)`.

### 3.3 Fees

Store fees:

* `fills.fee_usd`
  Optionally also log `pnl_ledger(kind='fee', ref=fill_id, pnl_usd = -fee)` for clarity.

### 3.4 Settlement (future-proof)

When you add settlement confirmation:

* insert `pnl_ledger(kind='adjustment', ref='settlement:<market_id>')` to finalize value
* ensures MTM does not “rewrite history”

---

## 4) Execution quality metrics (derived)

You can compute most metrics from `orders` + `fills`:

* Fill rate = filled orders / submitted orders
* Partial fill rate = count(status=PartiallyFilled) / submitted
* Time-to-fill distribution = (fill_ts - submit_ts)
* Cancel rate = canceled / submitted
* “Toxic fill proxy” = fill followed by adverse mid move within N seconds (computed offline)

Optional derived table (per-order rollups):

CREATE TABLE IF NOT EXISTS order_rollups (
  order_id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL,
  market_id INTEGER NOT NULL,
  strategy TEXT NOT NULL,
  qty_submitted REAL NOT NULL,
  qty_filled REAL NOT NULL,
  vwap REAL,
  fee_usd REAL,
  ts_first_fill_ms INTEGER,
  ts_last_fill_ms INTEGER,
  FOREIGN KEY(run_id) REFERENCES runs(run_id)
);


---

## 5) Queries you will actually use

### 5.1 PnL by strategy (daily)


SELECT
  date(ts_ms/1000, 'unixepoch') AS day,
  strategy,
  SUM(pnl_usd) AS pnl_usd
FROM pnl_ledger
WHERE run_id = :run_id
GROUP BY day, strategy
ORDER BY day, pnl_usd DESC;


### 5.2 Trace a weird trade end-to-end

SELECT
  i.intent_id, i.strategy, i.intent_kind, i.side, i.price, i.size, i.expected_value,
  a.approved, a.reason,
  o.order_id, o.client_order_id, o.status, o.ts_submitted_ms, o.ts_acked_ms,
  f.fill_id, f.ts_ms AS fill_ts, f.price AS fill_px, f.qty AS fill_qty, f.fee_usd
FROM strategy_intents i
LEFT JOIN arbiter_approvals a ON a.intent_id = i.intent_id
LEFT JOIN orders o ON o.intent_id = i.intent_id
LEFT JOIN fills f ON f.client_order_id = o.client_order_id
WHERE i.intent_id = :intent_id;


### 5.3 Execution latency stats


SELECT
  strategy,
  AVG(submit_latency_ms) AS avg_ms,
  MAX(submit_latency_ms) AS max_ms
FROM orders
WHERE run_id = :run_id AND submit_latency_ms IS NOT NULL
GROUP BY strategy;


---

## 6) Write-path rules (performance & correctness)

### 6.1 What to always log

* `raw_events`: at least user fills/order updates + WS connect/disconnect
* `strategy_intents` + `arbiter_approvals`
* `orders` + `fills`
* `incidents`: risk halts, stale state, exec errors

### 6.2 What to log conditionally (feature flags)

* Full market L2 `raw_events` (can be huge)
* High-frequency `snapshots`

Recommended flags:

* `LOG_L2_FULL=false` default
* `STORE_SNAPSHOTS=true` but only when strategies evaluated
* `STORE_FEATURES=true` (required for ML reproducibility)

### 6.3 Crash safety

* Inserts should never block the hot trading path indefinitely.
* Use buffered channels: trading loop emits events → storage writer task persists.

---

## 7) Upgrade path: SQLite → Postgres

Prepare now by:

* hiding DB access behind `crates/storage` methods
* using `sqlx` with feature flags (`sqlite` today, `postgres` later)
* avoiding SQLite-specific SQL outside storage crate

Migration approach:

* introduce `sqlx::migrate!()` folder
* run migrations at startup
* add Postgres tuning later (partitioning/BRIN indices) if volume demands it

---

## 8) Recommended minimal storage API (crates/storage)

Expose methods like:

* `log_raw_event(...)`
* `insert_snapshot(...)`
* `insert_intent(...)`
* `insert_approval(...)`
* `upsert_order(...)`
* `insert_fill(...)`
* `insert_pnl_ledger(...)`
* `insert_incident(...)`
* `insert_plugin_signal(...)`

This keeps backend swaps contained.

---

## 9) Implementation checklist (dev-friendly)

* [ ] Insert `runs` row on startup (include git sha + config hash)
* [ ] Storage writer task (bounded queue + backpressure)
* [ ] Log every Intent + every Approval (including rejected)
* [ ] Use `client_order_id` for idempotency on ALL orders
* [ ] Join fills on `client_order_id` even if venue order_id changes
* [ ] Implement FIFO lots engine for realized PnL (separate module)
* [ ] Write MTM snapshots conservatively (bid-side marks)
* [ ] Register `feature_schemas` row when schema version changes
