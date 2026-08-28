# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
for contract upgrades.

> **Semantic versioning note:** Breaking changes to the contract ABI (function
> signatures, event topics, storage keys) increment the **major** version.
> Additive changes (new functions, new events, new optional memo fields)
> increment the **minor** version. Internal refactors and dependency bumps
> increment the **patch** version..

## [Unreleased]

### Added

- `dispute_stats` view returning global dispute counters (open, resolved, expired) (#750).
- `full_token_info` view returning name, symbol, decimal, total_supply, max_supply, and version (#747).
- `escrow_stats_for_depositor` view counting active, released, and refunded escrows per depositor (#740).
- Tests for `emergency_withdraw` escrow-funds protection (#744).
- Pre-commit hook (`make install-hooks`) running `cargo fmt` and `cargo clippy` before every commit.
- CHANGELOG.md tracking all significant changes per release.
- Inline doc comments explaining the purpose of each test scenario across all test files.
- Inline `///` rustdoc for every public function in `src/contract.rs` (one-line
  summary, `# Arguments`, `# Panics`, and `# Example`).
- Error code catalog (`docs/error-codes.md`) mapping every panic string to its
  module, cause, and recommended caller handling.
- Architecture document (`docs/architecture.md`) covering module responsibilities,
  data flow, storage layout, auth model, events, and integration points.
- `transfer_recurring_payer` — transfer a recurring payment to a new payer with
  both parties authenticated; the payer index is updated.
- Test coverage for recurring payer transfer, permit nonce replay protection,
  storage TTL lifetimes, and dividend/airdrop supply invariants.
- Recurring execution window (`set_recurring_execution_window`) — recurring
  executions past `last_charged + interval + window` panic with
  `ExecutionWindowExpired` so keepers cannot run stale payments.
- `add_to_whitelist_signed` — bulk whitelist (max 200 addresses) via a single
  signed message with admin-nonce replay protection.
- `freeze_until` — freeze an account until a specific ledger; the freeze
  auto-clears once the current ledger passes the expiry ledger.
- Test coverage for `freeze_until`, vesting schedules, the recurring execution
  window, and the signed bulk whitelist.
- `add_to_whitelist_batch` — whitelist up to 50 accounts in a single admin call.
- Cap `set_protocol_fee` at 500 bps (5%) to keep the protocol fee bounded.
- Events for `setup_recurring`, `execute_recurring`, and `cancel_recurring`
  (`rcr_set`, `rcr_exec`, `rcr_cnl`) so off-chain indexers can track recurring
  lifecycle changes.
- Test coverage for protocol fees, whitelist mode, `amend_recurring`, and event
  emission across state-changing functions.

### Changed.

- CONTRIBUTING.md updated with pre-commit hook setup instructions.
- Add revision snapshotting (`take_snapshot`, `get_snapshot_balance`,
  `snapshot_taken_at`) with timestamped capture for audit/recovery.
- Add `admin_settle_escrow` to let an admin settle an escrow to a winner while
  honoring outstanding liens.
- Add dispute appeals (`appeal_dispute`, `resolve_appeal`) with a bounded
  appeal window and arbiter-gated resolution.
- Add `expire_dispute` to release a dispute after the expiry window, and track
  resolver resolution statistics.
- `raise_dispute` now records the dispute open ledger and maintains the claimant
  dispute index.
- `execute_recurring` now authorizes the payer before each charge and anchors the
  schedule to the prior due date to prevent drift.
- `distribute_split` no longer loses the rounding remainder (dust) — the
  residual is awarded to the first recipient.

## [0.1.0] — 2026-Q1

Full-featured token contract with escrow, dispute resolution, split payments,
recurring payments, admin controls, freeze/clawback, batch operations, pause,
allowance index, transfer memo, metadata update, and TTL bump hardening.

### Added

- Token core: `initialize`, `mint`, `burn`, `burn_from`, `transfer`, `transfer_from`,
  `transfer_with_memo`, `approve`, `allowance`, `total_supply`, `balance`.
- Admin rotation (`set_admin`) and metadata update (`update_metadata`).
- Freeze/unfreeze, freeze_batch/unfreeze_batch.
- Clawback and clawback_batch.
- Escrow: create, release, refund, partial release, admin settle.
- Dispute resolution: open, resolve, resolve_with_note, appeal, history tracking.
- Split payments: create, distribute, cancel, bulk distribute, split-with-escrow,
  split-with-memo.
- Recurring payments: setup, execute, cancel, amend, pause/resume, payer index.
- Batch mint and batch transfer (up to 50 recipients).
- Event emission for all state-changing operations.
- Allowance spender index (`allowances_for_spender`).
- Token info view combining metadata + supply.
- Admin info view.
- Storage TTL bump hardening for balance, allowance, escrow, split, recurring,
  dispute, and instance keys.
- CI pipeline with `cargo test`, `cargo fmt --check`, and `cargo clippy`.
- Snapshot-based test coverage reporting.

[Unreleased]: https://github.com/Lead-Studios/veritix-contract/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Lead-Studios/veritix-contract/releases/tag/v0.1.0
