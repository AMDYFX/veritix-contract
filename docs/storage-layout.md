# Storage layout

Every value the contract persists is reached through a single
`DataKey` enum in `src/storage_types.rs`. There is exactly one such enum in the
crate, by design: a previous implementation carried two side by side and they
disagreed about where an account's balance lived.

If you need a new key, **extend `DataKey`**. Do not declare another enum, and
do not hand-build a storage key from a tuple or a string.

## Durability tiers

| Tier           | Bounded by                                                                       | Holds                                                                                                 |
| -------------- | -------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| **Instance**   | The contract's storage footprint, restored wholesale if the contract is archived | Fixed configuration: who the admin is, total supply, the supply cap                                   |
| **Persistent** | Grows with usage; each entry archived independently once its TTL lapses          | Per-account and per-record data: balances, allowances, counters, escrows, splits, schedules, disputes |

Instance storage is cheap to read and is restored as a unit, so it is reserved
for a small fixed set of configuration values. Anything whose count grows with
the number of users belongs in persistent storage.

## Key reference

Every `DataKey` variant currently defined in `src/storage_types.rs`. The fixed
configuration keys live in instance storage; everything that grows with usage is
persistent.

| `DataKey` variant | Description | Durability |
| --- | --- | --- |
| `Admin` | Address holding admin authority. | Instance |
| `PendingAdmin` | The administrator nominated by a two-step ownership transfer, if any. | Persistent |
| `AdminActiveAfterLedger` | The ledger from which `PendingAdmin` becomes the active `Admin`. | Persistent |
| `ClawbackCosigner` | The second signer required alongside the admin for a clawback. | Persistent |
| `InitializedAtLedger` | The ledger at which the contract was initialized. | Instance |
| `Balance(Address)` | Token balance of a single account. | Persistent |
| `BalanceOf(Address)` | Token balance key used by the balance module. | Persistent |
| `Allowance(Address, Address)` | Spending allowance granted by `owner` to `spender`. | Persistent |
| `AllowanceSpenders(Address)` | The spenders an owner currently has an allowance with, in grant order. | Persistent |
| `EscrowLocked(Address)` | The portion of an account's balance held by an active escrow. | Persistent |
| `Paused` | Whether the whole contract is paused. | Instance |
| `WhitelistEnabled` | Whether token holders must be whitelisted. | Instance |
| `Frozen(Address)` | Whether an account is frozen and cannot move tokens. | Persistent |
| `Authorized(Address)` | Whether an account is authorized to move tokens. Absent means authorized. | Persistent |
| `TotalSupply` | Total tokens in circulation. | Instance |
| `MaxSupply` | Hard supply cap. Unset or 0 means supply is uncapped. | Instance |
| `Name` | The token name. | Persistent |
| `Symbol` | The token symbol. | Persistent |
| `Decimals` | The number of decimals the token uses. | Persistent |
| `Escrow(u64)` | An escrow held under its id, for lifecycle and settlement tracking. | Persistent |
| `EscrowRecord(u32)` | An escrow record held under its id. | Persistent |
| `EscrowCount` | Number of escrows created so far (also the next escrow id). | Persistent |
| `EscrowValueLocked(Address)` | Total token amount still held across all active escrows for a given token. | Persistent |
| `Split(u64)` | A payment split held under its id. | Persistent |
| `RecurringHistory(u64)` | The retained execution history for a recurring schedule. | Persistent |
| `PayerRecurrings(Address)` | Recurring schedule ids indexed by payer. | Persistent |
| `PayeeRecurrings(Address)` | Recurring schedule ids indexed by payee. | Persistent |
| `Dispute(u64)` | The dispute record open over an escrow, keyed by escrow id. | Persistent |
| `Recurring(u64)` | An authorised recurring payment, keyed by recurring payment id. | Persistent |
| `RecurringCount` | Number of recurring schedules created so far (also the next id). | Persistent |
| `Arbiter` | The globally configured dispute arbiter (resolver). | Persistent |
| `MultiEscrow(u64)` | A multi-beneficiary escrow held under its id. | Persistent |
| `MultiEscrowCount` | Number of multi-escrows created so far (also the next id). | Persistent |
| `Counter(Address)` | A monotonically increasing per-address counter. | Persistent |
| `Snapshot(u64, Address)` | A holder's snapshot balance under a given snapshot id. | Persistent |

## TTL policy

Soroban entries expire. An entry that is never touched eventually falls below
the network's minimum TTL and is archived, after which reading it returns
nothing — the contract cannot tell "never existed" from "expired", and a
balance that reads as zero is indistinguishable from a drained account.

Two helpers in `src/storage_types.rs` implement the policy. Both are the only
supported way to extend an entry's life.

| Helper                     | Applies to           | Threshold                                                      | Extends by                                              |
| -------------------------- | -------------------- | -------------------------------------------------------------- | ------------------------------------------------------- |
| `bump_instance(e)`         | Instance storage     | `INSTANCE_LIFETIME_THRESHOLD` (518,400 ledgers, ~30 days)      | `INSTANCE_BUMP_AMOUNT` (2,592,000 ledgers, ~15 days)    |
| `bump_persistent(e, &key)` | One persistent entry | `PERSISTENT_LIFETIME_THRESHOLD` (2,073,600 ledgers, ~120 days) | `PERSISTENT_BUMP_AMOUNT` (4,752,000 ledgers, ~180 days) |

Ledger figures assume Stellar's 5-second ledger close.

### When to call them

- **`bump_instance`** at the top of any entry point that reads or writes
  instance storage — in practice every administrative function, since the
  admin key and the supply counters live there. An archived instance is more
  serious than an archived balance: the contract does not exist at all until
  it is restored.
- **`bump_persistent`** on **every read and every write** of a persistent key,
  not only on writes.

That second point is the one most easily got wrong. Bumping only on writes
looks sufficient until you consider a long-dormant escrow: nothing writes to it
between creation and release, so at release time the entry is already archived
and the contract reads a missing record. A read that returns a live value is
exactly the case where the owner still cares, which is why reads bump too.

## Adding a key

1. Add the variant to `DataKey` in `src/storage_types.rs`, grouped by concern.
2. Decide instance or persistent. If the number of entries grows with usage,
   it is persistent.
3. Update the key reference table above.
4. Use `bump_persistent` on every read and write of the new key, or
   `bump_instance` if it is instance storage.
5. If the key moves or renames an existing value, that is an upgrade-guide
   concern, not a feature — see `docs/upgrade-guide.md`