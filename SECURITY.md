# Security

## Key rules
- No keys in repo, ever
- No running third-party repos on machines with keys
- Signing is isolated to venue adapter + minimal memory residency

## Secrets handling
- Use .env on VPS with strict permissions (chmod 600)
- Prefer encrypted secrets manager later (SOPS/KMS/Vault)
- Use gitleaks in CI

## Network exposure
- /metrics binds to 127.0.0.1 only
- admin unix socket only
- no public dashboard until behind VPN/tunnel

## Supply-chain scanning
- cargo-audit in CI
- CodeQL in CI
- Dependabot enabled
- optional container scanning if dockerized

End.
