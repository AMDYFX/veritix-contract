# Security Policy

## Reporting a Vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Report privately via [GitHub Security Advisories](https://github.com/Lead-Studios/veritix-contract/security/advisories/new) or email the maintainers directly.

Include:
- Affected function(s) or module(s)
- Steps to reproduce or a proof-of-concept
- Potential impact

**Response timeline:**
- Acknowledgment within **48 hours**
- Triage and severity assessment within **5 days**
- Patch or mitigation within **14 days** for critical issues

## Scope.

**In scope:**
- All functions in `src/` and `veritixpay/contract/token/src/`
- Escrow logic: create, release, refund, partial release
- Admin controls: initialization, rotation, clawback
- Token operations: mint, burn, transfer, approve, freeze
- Authorization and access-control paths

**Out of scope:**
- The Stellar network and Soroban runtime itself
- Third-party wallet or tooling vulnerabilities
- Issues in upstream dependencies outside this codebase

## Known Limitations

These are intentional design trade-offs, not bugs:

| Limitation | Rationale |
|---|---|
| Single `admin` key with no multi-sig | Simplicity; multi-sig admin is planned |
| Escrow expiry checked by ledger sequence, not wall clock | Deterministic on-chain execution; ledger time can drift |
| `MAX_ESCROW_AMOUNT = i128::MAX / 100` cap per escrow | Prevents liquidity lock from a single bricked escrow |
| Rate-limit cooldown (300 s) on `create_escrow` | Anti-spam; may affect legitimate high-frequency use |
| No `burn_from` in the escrow contract | Tracked in a separate issue; not yet implemented |

## Bug Bounty

There is no formal bug bounty program at this time. Significant responsible disclosures will be credited in the Hall of Fame below.

## Hall of Fame

*No disclosures yet. Be the first.*
