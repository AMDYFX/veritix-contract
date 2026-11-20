use soroban_sdk::{contracttype, Address, Env, Vec};

/// Every persistent storage key the contract owns.
///
/// Keeping the key set in one enum means a storage layout question only ever
/// has one answer, and the layout can be dumped as a whole when auditing.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    /// Address holding admin authority.
    Admin,
    /// The administrator nominated by a two-step ownership transfer, if any.
    PendingAdmin,
    /// The ledger from which `PendingAdmin` becomes the active `Admin`.
    AdminActiveAfterLedger,
    /// The second signer required alongside the admin for a clawback.
    ClawbackCosigner,
    /// The ledger at which the contract was initialized.
    InitializedAtLedger,

    // --- Balances ------------------------------------------------------------
    /// Token balance key used by the balance module.
    BalanceOf(Address),
    /// Spending allowance granted by `owner` to `spender`.
    Allowance(Address, Address),
    /// The spenders an owner currently has an allowance with, in grant order.
    AllowanceSpenders(Address),
    /// The portion of an account's balance held by an active escrow.
    EscrowLocked(Address),
    /// Every account that currently holds a non-zero balance, in first-credit order.
    Holders,

    // --- Compliance ----------------------------------------------------------
    /// Whether the whole contract is paused.
    Paused,
    /// Whether token holders must be whitelisted.
    WhitelistEnabled,
    /// Whether an account is frozen and cannot move tokens.
    Frozen(Address),
    /// Whether an account is whitelisted.
    Whitelisted(Address),
    /// Whether an account is authorized to move tokens. Absent means authorized.
    Authorized(Address),

    // --- Supply --------------------------------------------------------------
    /// Total tokens in circulation.
    TotalSupply,
    /// Hard supply cap. Unset or 0 means supply is uncapped.
    MaxSupply,

    // --- Metadata ------------------------------------------------------------
    /// The token name.
    Name,
    /// The token symbol.
    Symbol,
    /// The number of decimals the token uses.
    Decimals,

    // --- Escrows and disputes ------------------------------------------------
    /// An escrow held under its id, for lifecycle and settlement tracking.
    Escrow(u64),
    /// An escrow held under its id (u32).
    EscrowRecord(u32),
    /// Number of escrows created so far (also the next escrow id).
    EscrowCount,
    /// Total token amount still held across all active escrows for a given token.
    EscrowValueLocked(Address),
    /// Escrow ids created by a depositor, in creation order.
    DepositorEscrows(Address),
    /// Escrow ids owed to a beneficiary, in creation order.
    BeneficiaryEscrows(Address),
    /// A payment split held under its id.
    Split(u64),
    /// The retained execution history for a recurring schedule.
    RecurringHistory(u64),
    /// Recurring schedule ids indexed by payer.
    PayerRecurrings(Address),
    /// Recurring schedule ids indexed by payee.
    PayeeRecurrings(Address),
    /// The dispute record open over an escrow, keyed by escrow id.
    Dispute(u64),
    /// An authorised recurring payment, keyed by recurring payment id.
    Recurring(u64),
    /// Number of recurring schedules created so far (also the next id).
    RecurringCount,
    /// The globally configured dispute arbiter (resolver).
    Arbiter,
    /// A multi-beneficiary escrow held under its id.
    MultiEscrow(u64),
    /// Number of multi-escrows created so far (also the next id).
    MultiEscrowCount,
    /// Monotonically increasing counter / nonce per address.
    Counter(Address),
    /// Dispute escrow ids indexed by claimant.
    ClaimantDisputes(Address),
    /// All escrow ids that have a dispute.
    DisputeIds,
    /// A holder's snapshot balance under a given snapshot id.
    Snapshot(u64, Address),
    /// The total balance captured across all holders under a given snapshot id.
    SnapshotTotal(u64),

    // --- Protocol fee ----------------------------------------------------------
    /// The configured protocol fee, in basis points.
    ProtocolFeeBps,
    /// The treasury address receiving protocol fees.
    ProtocolFeeTreasury,
}

/// Where an escrow is in its lifecycle.
///
/// `Active` is the only state an escrow can be created in; the other two are
/// terminal and mutually exclusive, and neither ever returns to `Active`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EscrowStatus {
    /// Funds are still held and the escrow can still be settled.
    Active,
    /// Frozen by an open dispute; release and refund must panic while set.
    Disputed,
    /// Settled in the beneficiary's favour.
    Released,
    /// Settled in the depositor's favour.
    Refunded,
}

/// One escrow: who deposited, who is owed, and under what deadline.
///
/// `amount` is the amount still held, not the amount originally deposited. A
/// partial settlement reduces it, and an escrow is settled only once it reaches
/// zero, so a record never has to be read alongside the event log to tell how
/// much is still owed.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowRecord {
    pub depositor: Address,
    pub beneficiary: Address,
    pub token: Address,
    pub amount: i128,
    pub deadline_ledger: u32,
    pub status: EscrowStatus,
    /// Ledger at which the escrow was created, for age and expiry checks.
    pub created_ledger: u32,
}

/// A payment split: one sender, many recipients, shares in basis points.
///
/// `shares_bps` is parallel to `recipients`: `shares_bps[i]` is the share owed
/// to `recipients[i]`, and the shares must total 10_000.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SplitRecord {
    /// Who created the split and receives the surplus on cancellation.
    pub sender: Address,
    /// The token being distributed.
    pub token: Address,
    /// The full amount the split covers.
    pub total_amount: i128,
    /// Who is to be paid, in share order.
    pub recipients: Vec<Address>,
    /// Basis-point share per recipient, parallel to `recipients`. Totals 10_000.
    pub shares_bps: Vec<u32>,
    /// Whether distribution has already happened. Guards against paying twice.
    pub distributed: bool,
}

/// A recurring charge authorised between a payer and a payee.
///
/// `next_execution` is the ledger at which the next charge becomes due;
/// `interval_ledgers` is the gap between successive charges.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecurringPayment {
    /// Who is charged.
    pub payer: Address,
    /// Who receives the charge.
    pub payee: Address,
    /// The token charged.
    pub token: Address,
    /// The amount charged on each execution, in the token's base unit.
    pub amount: i128,
    /// Ledgers between successive charges. Must be greater than zero.
    pub interval_ledgers: u32,
    /// Ledger at which the next charge becomes due.
    pub next_execution: u32,
    /// Whether the schedule is still authorised. Cleared on cancellation.
    pub active: bool,
    /// Whether the schedule is temporarily suspended. Survives resume.
    pub paused: bool,
}

/// One successful recurring charge, recorded for dispute review.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecurringExecution {
    /// Ledger at which the charge succeeded.
    pub ledger: u32,
    /// Amount charged, in the token's base unit.
    pub amount: i128,
}

/// Lifecycle state of a dispute over an escrow.
///
/// The order is the progression: `Open` may become `Appealed`, and `Appealed`
/// may settle into `Resolved`. `Expired` is the alternative terminal state for
/// a dispute nobody acted on.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisputeStatus {
    /// Raised and awaiting a ruling. The escrow stays frozen.
    Open,
    /// A ruling was handed down and the escrow settled. Terminal.
    Resolved,
    /// The losing party escalated after a ruling. The escrow stays frozen.
    Appealed,
    /// Nobody acted before the deadline, so the dispute was closed out.
    /// Terminal.
    Expired,
}

/// A dispute raised over a single escrow.
///
/// While a dispute is `Open` or `Appealed` the underlying escrow must not be
/// released or refunded; that is the whole point of raising one. A ruling
/// recorded by `resolve_dispute` moves the dispute to `Resolved` with the
/// payout blocked until the appeal window lapses; if the losing party appeals,
/// the dispute becomes `Appealed` and only `resolve_appeal` (a final, unappealable
/// ruling) may settle the escrow.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeRecord {
    /// Who raised the dispute.
    pub claimant: Address,
    /// The escrow this dispute is over.
    pub escrow_id: u32,
    /// Current lifecycle state.
    pub status: DisputeStatus,
    /// Ledger at which the dispute was raised, for expiry and age checks.
    pub opened_ledger: u32,
    /// The arbiter assigned to settle it, if one has been set.
    pub resolver: Option<Address>,
    /// Ledger at which the most recent ruling was handed down; `0` if no
    /// ruling has been recorded yet. Measured against the appeal window.
    pub resolved_ledger: u32,
    /// The party declared the winner by the most recent ruling.
    pub winner: Option<Address>,
    /// True once an appeal has been resolved. The ruling is then final and a
    /// second appeal on the same escrow must be rejected.
    pub is_final: bool,
}

/// Where a multi-beneficiary escrow is in its lifecycle.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MultiEscrowStatus {
    /// Funds are still held and the record can still be settled.
    Active,
    /// Every beneficiary was paid and the record is drained to zero.
    Released,
    /// The full deposit was returned to the depositor.
    Refunded,
}

/// One multi-beneficiary escrow: a single deposit split across many payees.
///
/// `beneficiaries[i]` is owed `amounts[i]`; the two vectors are always the
/// same length, and `amounts` always sums to `total` at creation. Settlement
/// is atomic — either every leg is paid or the whole call reverts — so a
/// partial payout can never strand the record.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MultiEscrowRecord {
    /// Who funded the escrow and receives it back on refund.
    pub depositor: Address,
    /// The token being held.
    pub token: Address,
    /// One entry per beneficiary, parallel to `amounts`.
    pub beneficiaries: Vec<Address>,
    /// One entry per beneficiary, parallel to `beneficiaries`.
    pub amounts: Vec<i128>,
    /// The full deposited total; `amounts` must sum to it at creation.
    pub total: i128,
    /// Current lifecycle state.
    pub status: MultiEscrowStatus,
}

/// Ledgers of remaining life below which instance storage is extended (~30 days).
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = 518_400;
/// How far instance storage is extended once it falls below the threshold.
pub const INSTANCE_BUMP_AMOUNT: u32 = 2_592_000;
/// Ledgers of remaining life below which a persistent entry is extended (~120 days).
pub const PERSISTENT_LIFETIME_THRESHOLD: u32 = 2_073_600;
/// How far a persistent entry is extended once it falls below the threshold.
pub const PERSISTENT_BUMP_AMOUNT: u32 = 4_752_000;

/// Extends the life of instance storage, as described in `docs/ttl.md`.
pub fn bump_instance(e: &Env) {
    e.storage()
        .instance()
        .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}

/// Extends the life of the persistent entry stored under `key`.
pub fn bump_persistent(e: &Env, key: &DataKey) {
    e.storage()
        .persistent()
        .extend_ttl(key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
}
