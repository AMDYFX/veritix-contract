# Veritix Token Contract ABI Reference

This reference documents callable functions in `veritixpay/contract/token/src/contract.rs`.

## Token
- `initialize(admin: Address, name: String, symbol: String, decimal: u32) -> ()`
- `mint(admin: Address, to: Address, amount: i128) -> ()`
- `burn(from: Address, amount: i128) -> ()`
- `burn_from(spender: Address, from: Address, amount: i128) -> ()`
- `transfer(from: Address, to: Address, amount: i128) -> ()`
- `transfer_with_memo(from: Address, to: Address, amount: i128, memo: Bytes) -> ()`
- `transfer_from(spender: Address, from: Address, to: Address, amount: i128) -> ()`
- `approve(from: Address, spender: Address, amount: i128, expiration_ledger: u32) -> ()`
- `allowance(from: Address, spender: Address) -> i128`
- `allowances_for_spender(spender: Address) -> Vec<Address>`
- `total_supply() -> i128`
- `balance(id: Address) -> i128`
- `name() -> String`
- `symbol() -> String`
- `decimals() -> u32`
- `token_info() -> TokenInfo`

## Admin
- `admin() -> Address`
- `admin_info() -> AdminInfo`
- `set_admin(new_admin: Address) -> ()`
- `update_metadata(admin: Address, name: Option<String>, symbol: Option<String>) -> ()`

## Freeze / Batch Admin
- `freeze(target: Address) -> ()`
- `unfreeze(target: Address) -> ()`
- `freeze_batch(admin: Address, targets: Vec<Address>) -> ()`
- `unfreeze_batch(admin: Address, targets: Vec<Address>) -> ()`
- `is_frozen(id: Address) -> bool`
- `clawback(admin: Address, from: Address, amount: i128) -> ()`
- `clawback_batch(admin: Address, targets: Vec<(Address, i128)>) -> ()`

## Escrow
- `create_escrow(depositor: Address, beneficiary: Address, amount: i128, expiry_ledger: u32) -> u32`
- `release_escrow(caller: Address, escrow_id: u32) -> ()`
- `refund_escrow(caller: Address, escrow_id: u32) -> ()`
- `get_escrow(escrow_id: u32) -> EscrowRecord`
- `escrow_count() -> u32`
- `admin_settle_escrow(admin: Address, escrow_id: u32, recipient: Address) -> ()`

## Dispute
- `open_dispute(claimant: Address, escrow_id: u32, resolver: Address) -> u32`
- `resolve_dispute(resolver: Address, dispute_id: u32, release_to_beneficiary: bool) -> ()`
- `get_dispute(dispute_id: u32) -> DisputeRecord`
- `dispute_count() -> u32`
- `raise_dispute(caller: Address, escrow_id: u32) -> ()`
- `appeal_dispute(caller: Address, escrow_id: u32) -> ()`
- `resolve_appeal(resolver: Address, escrow_id: u32, winner: Address) -> ()`
- `expire_dispute(caller: Address, escrow_id: u32) -> ()`
- `get_disputes_by_claimant(claimant: Address) -> Vec<u32>`
- `resolver_stats(resolver: Address) -> ResolverStats`

## Snapshot
- `take_snapshot(admin: Address, account: Address) -> ()`
- `get_snapshot_balance(account: Address) -> i128`
- `snapshot_taken_at(account: Address) -> u32`

## Splitter
- `create_split(sender: Address, recipients: Vec<SplitRecipient>, total_amount: i128) -> u32`
- `distribute(caller: Address, split_id: u32) -> ()`
- `cancel_split(caller: Address, split_id: u32) -> ()`
- `get_split(split_id: u32) -> SplitRecord`
- `split_count() -> u32`

## Recurring
- `setup_recurring(payer: Address, payee: Address, amount: i128, interval: u32) -> u32`
- `execute_recurring(recurring_id: u32) -> ()`
- `cancel_recurring(caller: Address, recurring_id: u32) -> ()`
- `get_recurring(recurring_id: u32) -> RecurringRecord`
- `recurring_count() -> u32`

## Authorization and Events
- Auth is enforced either by `Address::require_auth()` or `check_admin`.
- Events are emitted with `symbol_short!` keys and typed payloads in each module.
- Panic strings are source-of-truth in module implementations (`balance.rs`, `escrow.rs`, `dispute.rs`, `splitter.rs`, `recurring.rs`, `validation.rs`).
