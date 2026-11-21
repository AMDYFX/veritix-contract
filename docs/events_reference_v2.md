# Events reference

Events are published with `#[contractevent(data_format = "vec")]`. The first topic is the event name (shown below); the remaining topics and the data vector follow in the order listed. Generated from `src/events.rs`.

| Event | Topics (after the name) | Data | Description |
| --- | --- | --- | --- |
| `mint` | `to: Address` | `amount: i128` | New tokens were credited to `to` |
| `transfer` | `from: Address`, `to: Address` | `amount: i128` | Tokens moved from `from` to `to` |
| `clawback` | `from: Address` | `amount: i128` | Tokens were clawed back from `from` by the admin |
| `escrow_created` | `depositor: Address`, `beneficiary: Address` | `id: u32`, `token: Address`, `amount: i128`, `deadline_ledger: u32` | A new escrow was created |
| `escrow_released` | `beneficiary: Address` | `id: u32`, `amount: i128`, `remaining: i128` | `amount` of escrow `id` was paid out to its beneficiary |
| `escrow_refunded` | `depositor: Address` | `id: u32`, `amount: i128` | Escrow `id` was cancelled and `amount` returned to its depositor |
| `dispute_raised` | `claimant: Address` | `escrow_id: u64` | A dispute was raised over an escrow, freezing it until a ruling is final |
| `dispute_appealed` | `caller: Address` | `escrow_id: u32` | An appeal was filed on a dispute |
| `dispute_finalised` | `resolver: Address` | `escrow_id: u32`, `winner: Address` | A dispute was finalised and settled |
| `recurring_cancelled` | `caller: Address` | `recurring_id: u64` | A recurring payment schedule was cancelled |
| `multi_escrow_created` | `depositor: Address` | `id: u64`, `total: i128` | A multi-beneficiary escrow was created |
| `multi_escrow_released` | `depositor: Address` | `id: u64`, `total: i128` | A multi-beneficiary escrow was fully released |
| `multi_escrow_refunded` | `depositor: Address` | `id: u64`, `total: i128` | A multi-beneficiary escrow was refunded |
| `burn` | `from: Address` | `amount: i128` | Tokens were burned from `from` |
| `approve` | `from: Address`, `spender: Address` | `amount: i128`, `expiration_ledger: u32` | `from` approved `spender` to move up to `amount` until `expiration_ledger` |
| `allowances_revoked` | `owner: Address` | `count: u32` | `owner` revoked every allowance they had granted |
