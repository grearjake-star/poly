# Runbook (VPS)

## Deployment model
- Single VPS, pinned CPU governor/perf mode if possible
- traderd runs as systemd service
- /metrics bound to 127.0.0.1; scrape locally or via SSH tunnel
- admin unix socket local-only (/tmp/polymarket_bot.sock)

## Systemd unit (example)
[Unit]
Description=polymarket trader daemon
After=network.target

[Service]
Type=simple
User=trader
WorkingDirectory=/opt/polymarket-bot
EnvironmentFile=/opt/polymarket-bot/.env
ExecStart=/opt/polymarket-bot/bin/traderd
Restart=always
RestartSec=2
LimitNOFILE=1048576

[Install]
WantedBy=multi-user.target

## Incident response
### If PnL drawdown triggers
1) traderctl caps ... (tighten)
2) traderctl pause mm
3) traderctl flatten <market>
4) inspect logs + metrics: fills, slippage proxies, WS health

### If WS disconnects
- bot must auto-disable PlaceOrder when stale
- operator checks:
  - WS reconnect loop
  - timeouts
  - venue status
  - local network

## Backups
- SQLite DB snapshot daily (offsite)
- rotate logs

## Database migrations
- Migrations live in `crates/storage/migrations` and are applied automatically during `traderd` startup.
- For manual runs (pre-flight checks or CI), install `sqlx-cli` and run:
  ```bash
  cargo install sqlx-cli --no-default-features --features sqlite
  sqlx migrate run --source crates/storage/migrations --database-url sqlite://bot.db
  ```

End.

## SQLite path examples

- Unix: `--sqlite-path sqlite://bot.db`
- Windows: `--sqlite-path "sqlite://C:/poly/data/bot.db"`

Ensure the database directory is writable; it will be created if it does not exist.
