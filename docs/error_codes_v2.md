# Error codes

The contract signals failure by panicking with a message of the form `Code: details`. The `Code` prefix is stable and is what clients should match on; the details are for humans.

| Code | Raised in | Meaning |
| --- | --- | --- |
| `AlreadyInitialized` | `contract` | already initialized |
| `AmountAboveRemainder` | `escrow` | amount above remainder |
| `AppealCallerNotParty` | `dispute` | appeal caller not party |
| `AppealWindowClosed` | `dispute` | appeal window closed |
| `AppealWindowOpen` | `dispute` | appeal window open |
| `ArbiterNotSet` | `dispute` | arbiter not set |
| `BalanceOverflow` | `balance` | balance overflow |
| `BatchTooLarge` | `batch` | batch length exceeds the maximum |
| `DisputeAlreadyOpen` | `dispute` | dispute already open |
| `DisputeFinal` | `dispute` | dispute final |
| `DisputeNoResolver` | `dispute` | dispute no resolver |
| `DisputeNotAppealable` | `dispute` | dispute not appealable |
| `DisputeNotAppealed` | `dispute` | dispute not appealed |
| `DisputeNotExpired` | `dispute` | escrow deadline ledger has not passed yet |
| `DisputeNotFound` | `dispute` | dispute not found |
| `DisputeNotOpen` | `dispute` | dispute not open |
| `DisputeSelfArbitration` | `dispute` | a party cannot arbitrate their own dispute |
| `EmptyHolders` | `dividend` | at least one holder required |
| `EscrowDisputed` | `escrow` | escrow disputed |
| `EscrowIdOverflow` | `escrow` | escrow id overflow |
| `EscrowNotActive` | `dispute`, `escrow` | escrow not active |
| `EscrowNotFound` | `dispute`, `escrow` | escrow not found |
| `EscrowOverflow` | `escrow` | escrow overflow |
| `EscrowSameParty` | `escrow` | escrow same party |
| `EscrowUnderflow` | `escrow` | escrow underflow |
| `ExpirationInPast` | `allowance` | expiration in past |
| `Frozen` | `control` | frozen |
| `InsufficientAllowance` | `allowance` | insufficient allowance |
| `InsufficientBalance` | `balance` | insufficient balance |
| `InvalidAmount` | `allowance`, `validation` | invalid amount |
| `InvalidCosigner` | `admin` | co-signer must differ from the admin; both keys are meant to be independent |
| `MemoTooLarge` | `balance` | memo exceeds the allowed byte length |
| `MissingSnapshot` | `dividend` | holder has no snapshot |
| `MultiEscrowEmpty` | `multi_escrow` | multi escrow empty |
| `MultiEscrowLengthMismatch` | `multi_escrow` | multi escrow length mismatch |
| `MultiEscrowNotActive` | `multi_escrow` | multi escrow not active |
| `MultiEscrowNotFound` | `multi_escrow` | multi escrow not found |
| `MultiEscrowOverflow` | `multi_escrow` | multi escrow overflow |
| `MultiEscrowSumMismatch` | `multi_escrow` | multi escrow sum mismatch |
| `NoSnapshotTotal` | `dividend` | snapshot total is zero or missing |
| `NoHolders` | `airdrop` | holder set is empty |
| `NotAuthorized` | `authorization` | not authorized |
| `NotInitialized` | `admin` | not initialized |
| `Paused` | `control` | paused |
| `RecurringIdOverflow` | `recurring` | recurring id overflow |
| `RecurringInactive` | `recurring` | recurring inactive |
| `RecurringIntervalInvalid` | `recurring` | recurring interval invalid |
| `RecurringLedgerOverflow` | `recurring` | recurring ledger overflow |
| `RecurringNotDue` | `recurring` | recurring not due |
| `RecurringNotFound` | `recurring` | recurring not found |
| `RecurringPaused` | `recurring` | recurring paused |
| `RecurringSameParty` | `recurring` | recurring same party |
| `SnapshotOverflow` | `dividend` | snapshot balances overflow i128 |
| `ShareTooSmall` | `airdrop` | total amount insufficient for distribution |
| `SplitAlreadyDistributed` | `splitter` | split already distributed |
| `SplitNotFound` | `splitter` | split not found |
| `SplitRecipientNotFound` | `splitter` | split recipient not found |
| `SupplyCapExceeded` | `balance` | supply cap exceeded |
| `SupplyOverflow` | `balance` | supply overflow |
| `SupplyUnderflow` | `balance` | burn would drive total supply below zero |
| `TooManyHolders` | `airdrop` | holder count exceeds the configured limit |
| `Unauthorized` | `admin`, `dispute`, `escrow`, `multi_escrow`, `splitter` | unauthorized |
| `WinnerMismatch` | `dispute` | ruling winner cannot be changed |
| `WinnerNotEscrowParty` | `dispute` | winner not escrow party |

The meaning column is derived from the code name; see the raising module for the exact condition.
