# Veritix Pay

![Coverage](https://img.shields.io/badge/coverage-llvm--cov%20(80%25%20min)-blue)

![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)
![Built With Rust](https://img.shields.io/badge/Built%20With-Rust-orange.svg)
![Network: Stellar / Soroban](https://img.shields.io/badge/Network-Stellar%20%2F%20Soroban-7b5ea7.svg)
![Status: In Development](https://img.shields.io/badge/Status-In%20Development-yellow.svg)

On-chain payment infrastructure for the Veritix ticketing platform, built with Rust and Soroban on the Stellar network..

---

## Overview.

Veritix Pay is the payment layer of a blockchain-based ticketing system. It lives entirely on-chain as a Soroban smart contract and handles all financial operations that power the Veritix platform — from a fan buying a ticket to an organizer receiving settlement funds after an event.

The contract handles **token transfers**, **escrow for ticket purchases**, **recurring payments**, **payment splitting between organizers, artists, and venues**, and **dispute resolution** when something goes wrong. Each of these is designed as a focused, composable module that shares a common storage layout and authorization model.

Escrowed funds are held temporarily on the contract's own ledger balance. When an escrow is created, tokens move from the depositor into the contract address, stay there while the escrow is unresolved, and move back out only on release or refund. This means the contract address can hold a real token balance during escrow flows without increasing total supply.

This project is actively being built in the open. Core token primitives, escrow logic, recurring payments, splitting, and dispute resolution are implemented and tested. Some draft modules exist in `src/` but are not yet wired into the crate. Contributors can claim open GitHub Issues to extend and improve existing modules. See the [Contributing](#contributing) section below.

---

## Why Stellar & Soroban

- **Deterministic execution and predictable fees** — no gas spikes or unpredictable costs
- **Fast finality** — Stellar's 5-second finality is suitable for real-time ticket validation and payment confirmation
- **Rust-based safety guarantees** — Soroban contracts are written in Rust, giving strong compile-time correctness checks
- **Native Stellar ecosystem integration** — works directly with Stellar assets and accounts, no bridges required

---

## Contract Modules

The entry point is `veritixpay/contract/token/src/lib.rs`. Modules compiled into the crate are listed below.

| Module | File | Status | Description |
|--------|------|--------|-------------|
| Token Core | `contract.rs` | Compiled | Mint, burn, transfer, approve, clawback, freeze |
| Admin | `admin.rs` | Compiled | Admin address controls and rotation |
| Allowance | `allowance.rs` | Compiled | Third-party spending approvals |
| Balance | `balance.rs` | Compiled | Ledger balance reads/writes |
| Escrow | `escrow.rs` | Compiled | Create, release, and refund escrow holds |
| Freeze | `freeze.rs` | Compiled | Regulatory account blocking |
| Metadata | `metadata.rs` | Compiled | Token name, symbol, decimals |
| Storage Types | `storage_types.rs` | Compiled | Shared `DataKey` enum and struct definitions |
| Recurring Payments | `recurring.rs` | Draft | Set up and execute recurring charges (not yet exposed in `contract.rs`) |
| Payment Splitter | `splitter.rs` | Draft | Split a payment between multiple parties (not yet exposed in `contract.rs`) |
| Dispute Resolution | `dispute.rs` | Draft | Open and resolve payment disputes (not yet exposed in `contract.rs`) |

---

## Getting Started

### Prerequisites

| Tool | Notes | Install |
|------|-------|---------|
| Rust (stable) | Required to compile Soroban contracts | https://rustup.rs |
| wasm32v1 target | Required build target | `rustup target add wasm32v1-none` |
| Stellar CLI (latest) | For building and deploying | `cargo install stellar-cli` |

Verify your setup:

```bash
rustc --version
stellar --version
```

### Clone and Build

```bash
git clone https://github.com/Lead-Studios/veritix-contract.git
cd veritix-contract/veritixpay/contract/token

make preflight  # verify all required tools are installed
make build     # compile to WASM (requires stellar CLI)
make test      # run tests
make fmt       # format code
make clean     # remove build artifacts
```

> **Note:** Run `make preflight` first to verify your setup. It checks for stellar CLI, Rust, and the `wasm32v1-none` target.

### Contract Address

The deployed contract address will be written to the repository's deployment output and can also be stored in `.contract-id` after running `scripts/deploy.sh`.

### Test Coverage

Closes #554: CI enforces a minimum of 80% line coverage via `cargo-llvm-cov`
(`--fail-under-lines 80`) on every PR — see the badge at the top of this
README and the `Coverage (min 80%)` step in `.github/workflows/ci.yml`.

### Binary Size

Closes #556: CI fails any PR whose compiled WASM exceeds the Soroban 100KB
deployment limit — see the `Check WASM size (max 100KB)` step in
`.github/workflows/ci.yml`, which prints the current size on every build.

---

## Project Structure

```
veritixpay/
├── contract/
│   └── token/
│       ├── src/
│       │   ├── lib.rs              # Crate entry point — module declarations
│       │   ├── contract.rs         # Public Soroban interface (VeritixToken)
│       │   ├── admin.rs            # Admin storage and rotation
│       │   ├── allowance.rs        # Spending approvals
│       │   ├── balance.rs          # Ledger balance helpers
│       │   ├── escrow.rs           # Escrow logic
│       │   ├── freeze.rs           # Account freeze/unfreeze
│       │   ├── metadata.rs         # Token metadata
│       │   ├── storage_types.rs    # Shared DataKey enum and structs
│       │   ├── test.rs             # Compiled unit tests
│       │   ├── escrow_test.rs      # Escrow-specific tests
│       │   └── admin_test.rs       # Admin rotation tests
│       ├── Cargo.toml
│       └── Makefile
├── Cargo.toml
├── Cargo.lock
├── .gitignore
└── README.md
```

---

## Contributing

Contributions are welcome and actively encouraged. This project is structured so that each contract module is an independent issue that any contributor can pick up.

To get started:
1. Browse [open issues](https://github.com/Lead-Studios/veritix-contract/issues)
2. Comment on one to get assigned
3. Branch from `main`, build your module, write tests, and open a PR

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full guide — including project structure, how to add a module, storage conventions, authorization rules, and the PR checklist.

---

## Open Source Wave

This project is part of an active open-source funding wave on [Drips Network](https://www.drips.network/). Contributors who build meaningful features may be eligible for rewards. Build something real, on a real chain, with real incentives.

---

## Related Repositories

- **Backend:** https://github.com/Lead-Studios/veritix-backend
- **Web client:** https://github.com/Lead-Studios/veritix-web

---

## License

MIT

## Smart Contract Optimization & Deployment

The Veritix smart contracts are compiled to target `wasm32v1-none` for deployment on the Stellar Soroban network. 

* **Binary Size Limit:** $\le 100\text{ KB}$ (Enforced automatically via CI checks)
* **Current Baseline Size:** ~64 KB (Optimized release build)

# Veritix Smart Contracts

[![CI Status](https://github.com/Lead-Studios/veritix-contract/actions/workflows/ci.yml/badge.svg)](https://github.com/Lead-Studios/veritix-contract/actions/workflows/ci.yml)
[![Coverage Status](https://codecov.io/gh/Lead-Studios/veritix-contract/branch/develop/graph/badge.svg)](https://codecov.io/gh/Lead-Studios/veritix-contract)

## Testing & Code Coverage

Test coverage is continuously monitored using `cargo tarpaulin`. 

* **Minimum Coverage Threshold:** $80\%$ (Enforced strictly via CI pipeline)