CREATE TABLE IF NOT EXISTS schema_meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

INSERT OR IGNORE INTO schema_meta (key, value) VALUES ('schema_version', '1');

CREATE TABLE IF NOT EXISTS runs (
  run_id TEXT PRIMARY KEY,
  started_at_ms INTEGER NOT NULL,
  git_sha TEXT,
  config_hash TEXT,
  host TEXT,
  notes TEXT
);

CREATE TABLE IF NOT EXISTS raw_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id TEXT NOT NULL,
  ts_ms INTEGER NOT NULL,
  source TEXT NOT NULL,
  topic TEXT NOT NULL,
  market_id INTEGER,
  payload_json TEXT NOT NULL,
  FOREIGN KEY(run_id) REFERENCES runs(run_id)
);

CREATE INDEX IF NOT EXISTS idx_raw_events_run_ts ON raw_events(run_id, ts_ms);
CREATE INDEX IF NOT EXISTS idx_raw_events_market_ts ON raw_events(market_id, ts_ms);
CREATE INDEX IF NOT EXISTS idx_raw_events_source_topic_ts ON raw_events(source, topic, ts_ms);

CREATE TABLE IF NOT EXISTS snapshots (
  snapshot_id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id TEXT NOT NULL,
  ts_ms INTEGER NOT NULL,
  market_id INTEGER NOT NULL,
  best_bid_px REAL,
  best_bid_qty REAL,
  best_ask_px REAL,
  best_ask_qty REAL,
  spread REAL,
  yes_qty REAL NOT NULL,
  no_qty REAL NOT NULL,
  net_exposure_usd REAL NOT NULL,
  can_trade INTEGER NOT NULL,
  drawdown_halt INTEGER NOT NULL,
  crowding_score REAL NOT NULL,
  toxicity_score REAL NOT NULL,
  spread_compression REAL NOT NULL,
  feature_schema_version INTEGER NOT NULL,
  features_json TEXT NOT NULL,
  FOREIGN KEY(run_id) REFERENCES runs(run_id)
);

CREATE INDEX IF NOT EXISTS idx_snapshots_run_market_ts ON snapshots(run_id, market_id, ts_ms);

CREATE TABLE IF NOT EXISTS strategy_intents (
  intent_id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id TEXT NOT NULL,
  ts_ms INTEGER NOT NULL,
  snapshot_id INTEGER NOT NULL,
  strategy TEXT NOT NULL,
  market_id INTEGER NOT NULL,
  intent_kind TEXT NOT NULL,
  side TEXT,
  price REAL,
  size REAL,
  urgency TEXT NOT NULL,
  ttl_ms INTEGER NOT NULL,
  expected_value REAL NOT NULL,
  confidence REAL NOT NULL,
  risk_cost REAL NOT NULL,
  tags_json TEXT NOT NULL,
  rationale_json TEXT,
  FOREIGN KEY(run_id) REFERENCES runs(run_id),
  FOREIGN KEY(snapshot_id) REFERENCES snapshots(snapshot_id)
);

CREATE INDEX IF NOT EXISTS idx_intents_run_ts ON strategy_intents(run_id, ts_ms);
CREATE INDEX IF NOT EXISTS idx_intents_market_ts ON strategy_intents(market_id, ts_ms);
CREATE INDEX IF NOT EXISTS idx_intents_strategy_ts ON strategy_intents(strategy, ts_ms);

CREATE TABLE IF NOT EXISTS arbiter_approvals (
  approved_id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id TEXT NOT NULL,
  ts_ms INTEGER NOT NULL,
  intent_id INTEGER NOT NULL,
  approved INTEGER NOT NULL,
  reason TEXT,
  owner_strategy TEXT,
  FOREIGN KEY(run_id) REFERENCES runs(run_id),
  FOREIGN KEY(intent_id) REFERENCES strategy_intents(intent_id)
);

CREATE INDEX IF NOT EXISTS idx_approvals_intent ON arbiter_approvals(intent_id);
CREATE INDEX IF NOT EXISTS idx_approvals_run_ts ON arbiter_approvals(run_id, ts_ms);

CREATE TABLE IF NOT EXISTS orders (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id TEXT NOT NULL,
  ts_submitted_ms INTEGER NOT NULL,
  approved_id INTEGER,
  intent_id INTEGER,
  strategy TEXT NOT NULL,
  market_id INTEGER NOT NULL,
  venue TEXT NOT NULL,
  order_id TEXT,
  client_order_id TEXT NOT NULL,
  status TEXT NOT NULL,
  side TEXT NOT NULL,
  limit_price REAL NOT NULL,
  qty REAL NOT NULL,
  ts_acked_ms INTEGER,
  ts_final_ms INTEGER,
  submit_latency_ms INTEGER,
  notes TEXT,
  FOREIGN KEY(run_id) REFERENCES runs(run_id),
  FOREIGN KEY(approved_id) REFERENCES arbiter_approvals(approved_id),
  FOREIGN KEY(intent_id) REFERENCES strategy_intents(intent_id)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_orders_client_oid ON orders(client_order_id);
CREATE INDEX IF NOT EXISTS idx_orders_order_id ON orders(order_id);
CREATE INDEX IF NOT EXISTS idx_orders_market_ts ON orders(market_id, ts_submitted_ms);
CREATE INDEX IF NOT EXISTS idx_orders_strategy_ts ON orders(strategy, ts_submitted_ms);

CREATE TABLE IF NOT EXISTS fills (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id TEXT NOT NULL,
  ts_ms INTEGER NOT NULL,
  venue TEXT NOT NULL,
  fill_id TEXT,
  order_id TEXT,
  client_order_id TEXT,
  market_id INTEGER NOT NULL,
  strategy TEXT NOT NULL,
  side TEXT NOT NULL,
  price REAL NOT NULL,
  qty REAL NOT NULL,
  fee_usd REAL DEFAULT 0.0,
  liquidity TEXT,
  raw_json TEXT,
  FOREIGN KEY(run_id) REFERENCES runs(run_id)
);

CREATE INDEX IF NOT EXISTS idx_fills_order_id ON fills(order_id);
CREATE INDEX IF NOT EXISTS idx_fills_client_oid ON fills(client_order_id);
CREATE INDEX IF NOT EXISTS idx_fills_market_ts ON fills(market_id, ts_ms);
CREATE INDEX IF NOT EXISTS idx_fills_strategy_ts ON fills(strategy, ts_ms);

CREATE TABLE IF NOT EXISTS pnl_ledger (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id TEXT NOT NULL,
  ts_ms INTEGER NOT NULL,
  market_id INTEGER NOT NULL,
  strategy TEXT,
  kind TEXT NOT NULL,
  ref TEXT,
  pnl_usd REAL NOT NULL,
  notes TEXT,
  FOREIGN KEY(run_id) REFERENCES runs(run_id)
);

CREATE INDEX IF NOT EXISTS idx_pnl_run_ts ON pnl_ledger(run_id, ts_ms);
CREATE INDEX IF NOT EXISTS idx_pnl_market_ts ON pnl_ledger(market_id, ts_ms);
CREATE INDEX IF NOT EXISTS idx_pnl_strategy_ts ON pnl_ledger(strategy, ts_ms);

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

CREATE TABLE IF NOT EXISTS plugin_signals (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id TEXT NOT NULL,
  ts_ms INTEGER NOT NULL,
  plugin TEXT NOT NULL,
  scope TEXT NOT NULL,
  market_id INTEGER,
  payload_json TEXT NOT NULL,
  FOREIGN KEY(run_id) REFERENCES runs(run_id)
);

CREATE INDEX IF NOT EXISTS idx_plugin_run_ts ON plugin_signals(run_id, ts_ms);
CREATE INDEX IF NOT EXISTS idx_plugin_market_ts ON plugin_signals(market_id, ts_ms);

CREATE TABLE IF NOT EXISTS incidents (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id TEXT NOT NULL,
  ts_ms INTEGER NOT NULL,
  severity TEXT NOT NULL,
  kind TEXT NOT NULL,
  message TEXT NOT NULL,
  payload_json TEXT,
  FOREIGN KEY(run_id) REFERENCES runs(run_id)
);

CREATE INDEX IF NOT EXISTS idx_incidents_run_ts ON incidents(run_id, ts_ms);

CREATE TABLE IF NOT EXISTS feature_schemas (
  feature_schema_version INTEGER PRIMARY KEY,
  created_at_ms INTEGER NOT NULL,
  description TEXT NOT NULL,
  features_json TEXT NOT NULL
);
