# Upstreams (Dependencies, References, and Fork Policy)

This project is a Rust-first, execution-safe, multi-strategy Polymarket CLOB trading system.
We intentionally separate:
- **runtime dependencies** (pinned, minimal surface)
- **reference repos** (read-only patterns, never run with keys)
- **research repos** (ML/RL experimentation, separate repo)

---

## 0) Policy: dependency vs fork vs reference

### Use as a Cargo dependency (preferred)
Use upstream libraries as *libraries*, pinned to tags/releases.
- Pros: easy updates, minimal drift
- Cons: you adapt to API changes

**Rule:** do NOT fork unless upstream blocks core functionality.

### Track as reference (submodule or docs-only)
Bots/framework repos are often useful for design patterns but unsafe to import wholesale.
- Treat as “read the code, steal the ideas, rewrite safely.”

**Rule:** do not run reference repos on a machine that has production keys.

### Separate research repo
RL/ML training belongs in a separate repository (e.g., `polymarket-lab`), exporting model artifacts for Rust inference.

---

## 1) Primary runtime dependency (Rust)

### Polymarket Rust CLOB Client
Repo: https://github.com/Polymarket/rs-clob-client
Use: execution plumbing, REST + WS, auth, typed request/response scaffolding.

Pin to release tag:
- Current pin recommendation: `v0.3.1` (update only via explicit upgrade PR)

Cargo:
```toml
rs-clob-client = { git = "https://github.com/Polymarket/rs-clob-client", tag = "v0.3.1" }
Upgrade procedure:

Open PR “Upgrade rs-clob-client to vX.Y.Z”

Update pin + changelog notes

Run full replay/regression suite

Deploy to canary VPS only after 48h shadow validation

2) Polymarket official clients + docs (reference + spec by example)
Python CLOB client (reference)
Repo: https://github.com/Polymarket/py-clob-client
Use: “spec by example” for endpoints, auth flow, and order formats.

TypeScript CLOB client (reference)
Repo: https://github.com/Polymarket/clob-client
Use: examples + cross-checking request shapes.

Polymarket developer docs (source of truth)
CLOB Introduction (hybrid off-chain matching / on-chain settlement):
https://docs.polymarket.com/developers/CLOB/introduction

Authentication:
https://docs.polymarket.com/developers/CLOB/authentication

Endpoints (REST + Data API):
https://docs.polymarket.com/quickstart/introduction/endpoints

WebSocket overview (channels: market / user):
https://docs.polymarket.com/developers/CLOB/websocket/wss-overview

Market channel:
https://docs.polymarket.com/developers/CLOB/websocket/market-channel

User channel:
https://docs.polymarket.com/developers/CLOB/websocket/user-channel

3) Official-ish Polymarket bots/frameworks (reference only)
Polymarket Market Maker Keeper (Python)
Repo: https://github.com/Polymarket/poly-market-maker
Use: operational patterns:

cancel/replace loops

diffing desired orders vs open orders

SIGTERM safe shutdown

Polymarket Agents (Python)
Repo: https://github.com/Polymarket/agents
Use: plugin/data sourcing architecture patterns, NOT for low-latency execution.

4) Community bots (reference only; audit required)
warproxxx/poly-maker (Python MM bot)
Repo: https://github.com/warproxxx/poly-maker
Use: MM patterns + risk controls + operational lessons.

warproxxx/poly_data (data pipeline)
Repo: https://github.com/warproxxx/poly_data
Use: data logging/structuring ideas (do not import code blindly).

lorine93s/polymarket-market-maker-bot
Repo: https://github.com/lorine93s/polymarket-market-maker-bot
Use: inventory/risk/MM structure patterns (reference only).

5) RL / Cross-market state fusion (research only; separate repo)
humanplane/cross-market-state-fusion
Repo: https://github.com/humanplane/cross-market-state-fusion
Use: fused-state feature ideas, training journal, evaluation approach.

Related model page:

https://huggingface.co/HumanPlane/LACUNA

Policy:

Never mix research code paths into production execution code.

Export only model artifacts + feature schema into production (Rust inference).

6) Security scanning + safe import rules
Required scanners for THIS repo
cargo-audit (Rust deps)

gitleaks (secret scanning)

GitHub CodeQL (static analysis)

Optional container scan (Trivy) if you deploy as Docker

“Safe to fork” checklist (any external bot repo)
Before running ANY third-party repo:

Run in an isolated container VM

No keys, no wallets, no RPC creds

Inspect for:

outbound network calls to unknown hosts

key exfiltration patterns

obfuscated code

suspicious dependency pins

Only port patterns, not code

7) Version pinning + maintenance rules
Pinned dependencies change only in dedicated PRs.

Every upgrade requires:

compile + unit tests

replay regression suite

shadow run on canary VPS

documented changes in CHANGELOG.md
