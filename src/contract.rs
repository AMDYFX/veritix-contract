use crate::storage_types::{
    ContractInfo, DataKey, DisputeStats, EscrowDepositorStats, FullTokenInfo, RecurringExecution,
    RecurringPayment, ResolverStats, VestingRecord,
};
use crate::validation::require_positive_amount;
use crate::{
    admin, allowance, balance, dispute, escrow, multi_escrow, permit, recurring, snapshot,
    whitelist,
};
use soroban_sdk::{
    contract, contractimpl, contracttype, token, Address, Bytes, BytesN, Env, String, Vec,
};

// #573: Airdrop holder set tracking helper
fn track_holder_for_airdrop(e: &Env, addr: &Address) {
    let mut holders: Vec<Address> = e
        .storage()
        .persistent()
        .get(&DataKey::HolderSet)
        .unwrap_or_else(|| Vec::new(e));

    // Check if already present
    for i in 0..holders.len() {
        if holders.get(i).unwrap() == *addr {
            return;
        }
    }

    holders.push_back(addr.clone());
    e.storage().persistent().set(&DataKey::HolderSet, &holders);
    let count: u32 = e
        .storage()
        .persistent()
        .get(&DataKey::HolderCount)
        .unwrap_or(0);
    e.storage()
        .persistent()
        .set(&DataKey::HolderCount, &(count + 1));
}

pub trait VeriTixPayTrait {
    /// Initializes the contract, setting the contract admin and recording the
    /// initialization ledger. Must be called exactly once before any other
    /// state-changing operation.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `admin` — address granted admin privileges.
    ///
    /// # Panics
    /// - `AlreadyInitialized: contract state is locked` if `initialize` (or
    ///   `initialize_with_max_supply`) has already been called.
    ///
    /// # Example
    /// ```ignore
    /// contract.initialize(&env, &admin);
    /// ```
    fn initialize(e: Env, admin: Address);

    /// Initializes the contract with a hard supply cap. After the cap is set,
    /// `mint` and `mint_batch` cannot push `total_supply` above it.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `admin` — address granted admin privileges.
    /// - `max_supply` — maximum allowed total supply; `0` disables the cap.
    ///
    /// # Panics
    /// - `AlreadyInitialized: contract state is locked` if the contract was
    ///   already initialized.
    fn initialize_with_max_supply(e: Env, admin: Address, max_supply: i128);

    // ── SEP-41 Token Interface ────────────────────────────────────────────────
    /// Returns the token name (`"VeriTix"`).
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    fn name(e: Env) -> soroban_sdk::String;

    /// Returns the token symbol (`"VTX"`).
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    fn symbol(e: Env) -> soroban_sdk::String;

    /// Returns the token decimal places (`7`).
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    fn decimals(e: Env) -> u32;

    /// Returns the token balance held by an account.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `account` — the address to query.
    fn balance(e: Env, account: Address) -> i128;

    /// Returns the total token supply currently in circulation.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    fn total_supply(e: Env) -> i128;

    /// Mints `amount` tokens to `to`. Admin-only.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `admin` — the contract admin (authenticated).
    /// - `to` — recipient of the newly minted tokens.
    /// - `amount` — number of raw tokens to mint.
    ///
    /// # Panics
    /// - `Amount must be strictly positive` if `amount <= 0`.
    /// - `Unauthorized: caller is not the contract admin` if `admin` is not the
    ///   stored admin.
    /// - `SupplyCap: minting would exceed max supply of {max}` when a max
    ///   supply is configured and would be exceeded.
    /// - `supply overflow` if the total supply arithmetic overflows.
    ///
    /// # Example
    /// ```ignore
    /// contract.mint(&env, &admin, &user, &1_000_000);
    /// ```
    fn mint(e: Env, admin: Address, to: Address, amount: i128);

    /// Burns (destroys) `amount` tokens from `from`. The caller must be `from`.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `from` — the holder whose tokens are burned (authenticated).
    /// - `amount` — number of raw tokens to burn.
    ///
    /// # Panics
    /// - `Amount must be strictly positive` if `amount <= 0`.
    /// - `insufficient balance` if `from` holds fewer tokens than `amount`.
    ///
    /// # Example
    /// ```ignore
    /// contract.burn(&env, &user, &500_000);
    /// ```
    fn burn(e: Env, from: Address, amount: i128);

    /// Clawbacks (confiscates) `amount` tokens from `from`. Admin-only.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `admin` — the contract admin (authenticated).
    /// - `from` — the account whose tokens are taken.
    /// - `amount` — number of raw tokens to claw back.
    ///
    /// # Panics
    /// - `Amount must be strictly positive` if `amount <= 0`.
    /// - `Unauthorized: caller is not the contract admin` if `admin` is not the
    ///   stored admin.
    /// - `insufficient balance` if `from` holds fewer tokens than `amount`.
    fn clawback(e: Env, admin: Address, from: Address, amount: i128);

    // ── Escrow ────────────────────────────────────────────────────────────────
    /// Creates an escrow, pulling `amount` tokens from the depositor into the
    /// contract until release, refund, or resolution. This is the core
    /// ticket-purchase primitive.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `depositor` — party funding the escrow (authenticated).
    /// - `beneficiary` — party that receives the funds on release.
    /// - `token` — address of the token contract being escrowed.
    /// - `amount` — token amount to lock (must be >= `MIN_ESCROW_AMOUNT`).
    /// - `expiry_ledger` — ledger after which the escrow can be refunded.
    /// - `memo` — arbitrary tag (e.g. ticket UUID); max 64 bytes.
    ///
    /// # Returns
    /// The new escrow ID.
    ///
    /// # Panics
    /// - `amount must be greater than zero` if `amount <= 0`.
    /// - `RateLimitExceeded: please wait before creating another escrow` if the
    ///   depositor creates escrows faster than the cooldown allows.
    /// - `MemoTooLong: memo cannot exceed 64 bytes` if the memo is too long.
    /// - `AmountTooSmall: escrow amount must be at least {n} tokens` if the
    ///   escrow is below the anti-spam minimum.
    /// - `expiry_ledger must be in the future` if the expiry is not strictly
    ///   after the current ledger.
    /// - `TooManyEscrows: depositor has reached the active escrow limit` if the
    ///   depositor exceeds `MAX_ESCROWS_PER_DEPOSITOR`.
    ///
    /// # Example
    /// ```ignore
    /// let id = contract.create_escrow(&env, &buyer, &organizer, &token, &50_000_000, &ledger + 1000, &memo);
    /// ```
    fn create_escrow(
        e: Env,
        depositor: Address,
        beneficiary: Address,
        token: Address,
        amount: i128,
        expiry_ledger: u32,
        memo: Bytes, // #175
    ) -> u32;

    /// Releases a fully-escrowed amount to the beneficiary. Callable by the
    /// depositor or the contract admin. Honors active liens and deducts any
    /// configured protocol fee before paying out.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `caller` — the depositor or admin (authenticated).
    /// - `escrow_id` — escrow to release.
    ///
    /// # Panics
    /// - `escrow not found` if the escrow does not exist.
    /// - `already released` if the escrow was already released.
    /// - `already refunded` if the escrow was already refunded.
    /// - `not authorised to release` if the caller is neither depositor nor admin.
    /// - `escrow has expired` if the current ledger is past the expiry.
    /// - `nothing left to release` if nothing remains to pay out.
    /// - `treasury not set` if a protocol fee is configured without a treasury.
    ///
    /// # Example
    /// ```ignore
    /// contract.release_escrow(&env, &depositor, &escrow_id);
    /// ```
    fn release_escrow(e: Env, caller: Address, escrow_id: u32);

    /// Partially releases `amount` from an escrow to the beneficiary. The
    /// caller must be the beneficiary.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `caller` — the beneficiary (authenticated).
    /// - `escrow_id` — escrow to partially release.
    /// - `amount` — amount to release this call.
    ///
    /// # Panics
    /// - `escrow not found` if the escrow does not exist.
    /// - `already refunded` / `already fully released` if the escrow is settled.
    /// - `only the beneficiary can partially release` if the caller is not the
    ///   beneficiary.
    /// - `escrow has expired` if the current ledger is past the expiry.
    /// - `release amount must be greater than zero` if `amount <= 0`.
    /// - `release amount exceeds remaining balance` if `amount` is too large.
    fn release_partial_escrow(e: Env, caller: Address, escrow_id: u32, amount: i128); // #174

    /// Refunds the un-released remainder of an escrow back to the depositor.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `caller` — the depositor or admin (authenticated).
    /// - `escrow_id` — escrow to refund.
    ///
    /// # Panics
    /// - `escrow not found` if the escrow does not exist.
    /// - `cannot refund active dispute` if an open dispute is pending.
    /// - `already released` / `already refunded` if the escrow is settled.
    /// - `not authorised to refund` if the caller is neither depositor nor admin.
    /// - `nothing left to refund` if nothing remains refundable.
    fn refund_escrow(e: Env, caller: Address, escrow_id: u32);

    /// Returns the list of escrow IDs funded by `depositor`.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `depositor` — the depositor to query.
    fn get_escrows_by_depositor(e: Env, depositor: Address) -> Vec<u32>;

    /// Returns the list of escrow IDs payable to `beneficiary`.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `beneficiary` — the beneficiary to query.
    fn get_escrows_by_beneficiary(e: Env, beneficiary: Address) -> Vec<u32>;

    /// Returns the total value currently locked in all open escrows.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    fn escrowed_total(e: Env) -> i128;

    /// Returns escrow statistics (total value locked).
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    fn escrow_stats(e: Env) -> escrow::EscrowStats;

    /// Places a lien (secured claim) on an escrow for `lien_amount`. Caller
    /// must be the creditor.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `creditor` — party securing the claim (authenticated).
    /// - `escrow_id` — escrow to place the lien on.
    /// - `lien_amount` — secured amount.
    ///
    /// # Panics
    /// - `escrow already settled` if the escrow is released or refunded.
    /// - `only one lien at a time` if a lien already exists.
    /// - `lien amount must be positive` if `lien_amount <= 0`.
    /// - `lien amount exceeds escrow amount` if `lien_amount` is too large.
    fn place_lien(e: Env, creditor: Address, escrow_id: u32, lien_amount: i128);

    /// Clears an active lien on an escrow. Caller must be the depositor or the
    /// lienholder.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `caller` — the depositor or lienholder (authenticated).
    /// - `escrow_id` — escrow whose lien is cleared.
    ///
    /// # Panics
    /// - `escrow already settled` if the escrow is released or refunded.
    /// - `no active lien` if no lien exists.
    /// - `not authorized to clear lien` if the caller is not the depositor or
    ///   lienholder.
    fn clear_lien(e: Env, caller: Address, escrow_id: u32);

    /// Returns the full escrow record for `escrow_id`.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `escrow_id` — escrow to fetch.
    ///
    /// # Panics
    /// - `escrow not found` if the escrow does not exist.
    fn get_escrow(e: Env, escrow_id: u32) -> escrow::EscrowRecord;

    /// Returns `true` if the escrow has been released or refunded (settled).
    /// A non-existent escrow is reported as settled.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `escrow_id` — escrow to check.
    fn is_escrow_settled(e: Env, escrow_id: u32) -> bool;

    // ── Disputes ──────────────────────────────────────────────────────────────
    /// Returns the list of escrow IDs for disputes opened by `claimant`.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `claimant` — the claiming party.
    fn get_disputes_by_claimant(e: Env, claimant: Address) -> Vec<u32>;

    /// Sets the dispute arbiter. The current arbiter authenticates the call;
    /// once set, only the arbiter can resolve disputes and appeals.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `arbiter` — address that signs the call and becomes arbiter.
    fn set_arbiter(e: Env, arbiter: Address);

    /// Opens a dispute on an unsettled escrow, freezing the release/refund path
    /// until the arbiter resolves it. Only the depositor or beneficiary can raise.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `caller` — the depositor or beneficiary (authenticated).
    /// - `escrow_id` — escrow under dispute.
    ///
    /// # Panics
    /// - `maximum dispute count exceeded` if the per-escrow dispute limit is hit.
    /// - `escrow not found` if the escrow does not exist.
    /// - `escrow already settled` if the escrow is released or refunded.
    /// - `only depositor or beneficiary can raise dispute` if the caller is not
    ///   an escrow party.
    fn raise_dispute(e: Env, caller: Address, escrow_id: u32);

    /// Resolves an open dispute for an escrow, awarding the escrow to `winner`
    /// (depositor or beneficiary). The arbiter authenticates the call and any
    /// configured mediation fee is deducted.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `resolver` — the arbiter (authenticated).
    /// - `escrow_id` — escrow being resolved.
    /// - `winner` — the party awarded the funds.
    ///
    /// # Panics
    /// - `Unauthorized: only the arbiter can resolve disputes` if the resolver
    ///   is not the arbiter.
    /// - `escrow is not under dispute` if no dispute is open.
    /// - `escrow already settled` if the escrow is released or refunded.
    /// - `winner must be depositor or beneficiary` if the winner is not a party.
    /// - `arbiter not set` if no arbiter has been configured.
    fn resolve_dispute(e: Env, resolver: Address, escrow_id: u32, winner: Address);

    /// Returns `true` if an open dispute exists for the escrow.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `escrow_id` — escrow to check.
    fn is_dispute_open(e: Env, escrow_id: u32) -> bool;

    // ── Recurring Payments ────────────────────────────────────────────────────
    /// Sets up a recurring payment where `payer` is charged `amount` every
    /// `interval` ledgers, for up to `max_executions` times (`0` = unlimited).
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `payer` — the account funding each charge (authenticated).
    /// - `payee` — the account receiving each charge.
    /// - `token` — token used for the charges.
    /// - `amount` — amount charged per execution (must be positive).
    /// - `interval` — number of ledgers between charges.
    /// - `max_executions` — maximum number of charges (`0` = unlimited).
    ///
    /// # Returns
    /// The new recurring payment ID.
    ///
    /// # Panics
    /// - `amount must be positive` if `amount <= 0`.
    ///
    /// # Example
    /// ```ignore
    /// let rid = contract.setup_recurring(&env, &payer, &payee, &token, &1_000_000, &17280, &0);
    /// ```
    fn setup_recurring(
        e: Env,
        payer: Address,
        payee: Address,
        token: Address,
        amount: i128,
        interval: u32,
        max_executions: u32,
    ) -> u32;

    /// Executes a recurring payment that is due. Anyone may call (crank
    /// pattern); the payer is authorized on-chain before each charge.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `recurring_id` — recurring payment to execute.
    ///
    /// # Panics
    /// - `recurring not found` if the recurring record does not exist.
    /// - `recurring is not active` if the schedule was cancelled or finished.
    /// - `overflow` if the due-date arithmetic overflows.
    /// - `not yet due` if the interval has not elapsed.
    ///
    /// # Example
    /// ```ignore
    /// contract.execute_recurring(&env, &recurring_id);
    /// ```
    fn execute_recurring(e: Env, recurring_id: u32);

    /// Returns the execution audit log for a recurring payment.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `recurring_id` — recurring payment to query.
    fn get_recurring_history(e: Env, recurring_id: u32) -> Vec<RecurringPayment>;

    /// Returns `true` if a recurring payment is active and not paused.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `recurring_id` — recurring payment to check.
    fn is_recurring_active(e: Env, recurring_id: u32) -> bool;

    /// Batch-reads escrow records by ID, returning `None` for missing IDs.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `escrow_ids` — list of escrow IDs to fetch.
    ///
    /// # Panics
    /// - `batch size cannot exceed 50` if more than 50 IDs are passed.
    fn get_escrows_batch(e: Env, escrow_ids: Vec<u32>) -> Vec<Option<escrow::EscrowRecord>>;

    /// Returns how many ledgers an open escrow has been alive
    /// (0 once settled).
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `escrow_id` — escrow to query.
    ///
    /// # Panics
    /// - `escrow not found` if the escrow does not exist.
    fn get_escrow_age(e: Env, escrow_id: u32) -> u32;

    // ── Multi-escrow ──────────────────────────────────────────────────────────
    /// Creates a single escrow that distributes to multiple recipients with
    /// exact share amounts. Funds are pulled from the depositor as one batch.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `depositor` — party funding the escrow (authenticated).
    /// - `recipients` — list of `(address, share_amount)` pairs.
    /// - `token` — token being escrowed.
    /// - `expiry_ledger` — ledger after which it can be refunded.
    ///
    /// # Panics
    /// - `must have at least one recipient` if the list is empty.
    /// - `expiry_ledger must be in the future` if the expiry is not in the future.
    /// - `each recipient share must be greater than zero` if any share is `<= 0`.
    /// - `total amount must be greater than zero` if all shares sum to zero.
    fn create_multi_escrow(
        e: Env,
        depositor: Address,
        recipients: Vec<(Address, i128)>,
        token: Address,
        expiry_ledger: u32,
    ) -> u32;

    /// Releases a multi-escrow, paying each recipient their exact share.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `caller` — the depositor or admin (authenticated).
    /// - `multi_escrow_id` — multi-escrow to release.
    ///
    /// # Panics
    /// - `multi-escrow not found` if the record does not exist.
    /// - `already released` / `already refunded` if settled.
    /// - `not authorised to release` if the caller is not depositor or admin.
    /// - `multi-escrow has expired` if past the expiry ledger.
    fn release_multi_escrow(e: Env, caller: Address, multi_escrow_id: u32);

    /// Refunds a multi-escrow, returning the pooled amount to the depositor.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `caller` — the depositor or admin (authenticated).
    /// - `multi_escrow_id` — multi-escrow to refund.
    ///
    /// # Panics
    /// - `multi-escrow not found` if the record does not exist.
    /// - `already released` / `already refunded` if settled.
    /// - `not authorised to refund` if the caller is not depositor or admin.
    fn refund_multi_escrow(e: Env, caller: Address, multi_escrow_id: u32);

    /// Creates a ticket escrow: locks `ticket_price` from the buyer to the
    /// organizer until `event_ledger + 100`, tagged with `ticket_ref`.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `buyer` — ticket purchaser funding the escrow (authenticated).
    /// - `organizer` — ticket seller receiving funds on release.
    /// - `token` — token used for the ticket price.
    /// - `ticket_price` — price of the ticket (must be positive).
    /// - `event_ledger` — the event date ledger; escrow expires 100 ledgers later.
    /// - `ticket_ref` — ticket reference/order tag.
    ///
    /// # Panics
    /// - `Amount must be strictly positive` if `ticket_price <= 0`.
    fn ticket_escrow(
        e: Env,
        buyer: Address,
        organizer: Address,
        token: Address,
        ticket_price: i128,
        event_ledger: u32,
        ticket_ref: Bytes,
    ) -> u32;

    /// Splits a payment between an organizer, an artist, and the platform by
    /// basis points, then immediately distributes it. The sender authenticates.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `sender` — payer funding the split (authenticated).
    /// - `organizer` — first recipient.
    /// - `organizer_bps` — organizer share in basis points.
    /// - `artist` — second recipient.
    /// - `artist_bps` — artist share in basis points.
    /// - `platform` — third recipient (receives the remainder).
    /// - `token` — token being distributed.
    /// - `total_amount` — total amount to split.
    /// - `event_ledger` — event ledger used to derive the escrow expiry.
    ///
    /// # Panics
    /// - `Amount must be strictly positive` if `total_amount <= 0`.
    /// - `invalid basis points` if `organizer_bps + artist_bps >= 10000`.
    #[allow(clippy::too_many_arguments)]
    fn revenue_split(
        e: Env,
        sender: Address,
        organizer: Address,
        organizer_bps: u32,
        artist: Address,
        artist_bps: u32,
        platform: Address,
        token: Address,
        total_amount: i128,
        event_ledger: u32,
    ) -> u32;

    // ── Allowance ─────────────────────────────────────────────────────────────
    /// Approves `spender` to spend up to `amount` from `from` until
    /// `expiration_ledger`.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `from` — the allowance owner (authenticated).
    /// - `spender` — the account granted allowance.
    /// - `amount` — allowance amount.
    /// - `expiration_ledger` — ledger at which the allowance expires.
    ///
    /// # Panics
    /// - `Amount must be strictly positive` if `amount <= 0`.
    fn approve(e: Env, from: Address, spender: Address, amount: i128, expiration_ledger: u32);

    /// Transfers tokens from `from` to `to` using an approved allowance held by
    /// `spender`. Fails while the contract is paused.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `spender` — the allowance holder (authenticated).
    /// - `from` — the account being debited.
    /// - `to` — the account being credited.
    /// - `amount` — transfer amount.
    ///
    /// # Panics
    /// - `ContractPaused: contract is paused` if the contract is paused.
    /// - `Amount must be strictly positive` if `amount <= 0`.
    /// - `allowance expired` if the allowance has lapsed.
    /// - `insufficient allowance` if the allowance is less than `amount`.
    /// - `insufficient balance` if `from` lacks the funds.
    fn transfer_from(e: Env, spender: Address, from: Address, to: Address, amount: i128);

    // ── #451: Admin ownership ────────────────────────────────────────────────
    /// Proposes a new admin. The proposal is activated only after the new admin
    /// calls `accept_admin` (two-step rotation).
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `new_admin` — address being proposed.
    ///
    /// # Panics
    /// - `admin not set` if the contract is not initialized.
    /// - `cannot propose current admin` if `new_admin` is already the admin.
    fn transfer_ownership(e: Env, new_admin: Address);

    /// Accepts a pending admin proposal, completing the two-step rotation. The
    /// new admin becomes active after the delay period.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `new_admin` — the proposed admin (authenticated).
    ///
    /// # Panics
    /// - `no admin proposal pending` if no proposal exists.
    /// - `NotProposed: caller is not the proposed admin` if the caller was not
    ///   proposed.
    fn accept_admin(e: Env, new_admin: Address);

    /// Returns the ledger after which the (new) admin becomes active
    /// (`0` when no rotation is pending).
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    fn admin_active_after_ledger(e: Env) -> u32;

    // ── #452: Depositor escrowed value ───────────────────────────────────────
    /// Returns the total un-released value locked in escrows funded by
    /// `depositor`.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `depositor` — the depositor to query.
    ///
    /// # Panics
    /// - `escrow {n} not found` if an indexed escrow record is missing.
    fn escrowed_value_for_depositor(e: Env, depositor: Address) -> i128;

    // ── Pause ─────────────────────────────────────────────────────────────────
    /// Pauses or resumes the contract. While paused, transfers are blocked.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `admin` — the contract admin (authenticated).
    /// - `paused` — `true` to pause, `false` to resume.
    fn set_paused(e: Env, admin: Address, paused: bool);

    /// Returns whether the contract is currently paused.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    fn is_paused(e: Env) -> bool;
    fn contract_paused_for(e: Env) -> Option<u32>;

    // ── Permit / Nonce ────────────────────────────────────────────────────────
    /// Consumes the current nonce for `user`, replay-protecting subsequent
    /// signed operations. The caller must supply the current nonce value.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `user` — the nonce owner (authenticated).
    /// - `nonce` — the expected current nonce.
    ///
    /// # Panics
    /// - `InvalidNonce: expected {expected} but got {got}` if the nonce is
    ///   stale.
    fn permit(e: Env, user: Address, nonce: u32);

    /// Returns the current permit nonce for `user`.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `user` — the address to query.
    fn nonces(e: Env, user: Address) -> u64;

    // ── #453: Resolver stats ─────────────────────────────────────────────────
    /// Returns resolution statistics for a dispute resolver.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `resolver` — the arbiter/resolver to query.
    fn resolver_stats(e: Env, resolver: Address) -> ResolverStats;
    fn dispute_stats(e: Env) -> DisputeStats;
    fn full_token_info(e: Env) -> FullTokenInfo;
    fn escrow_stats_for_depositor(e: Env, depositor: Address) -> EscrowDepositorStats;

    // ── #454: Protocol fee stats ─────────────────────────────────────────────
    /// Returns protocol fee configuration: fee basis points, treasury address,
    /// and total fees collected so far.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    fn protocol_fee_stats(e: Env) -> (u32, Address, i128);

    /// Admin-only emergency withdrawal of non-escrowed funds held by the
    /// contract to `recipient`. Cannot touch escrowed value.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `admin` — the contract admin (authenticated).
    /// - `recipient` — destination of the withdrawn funds.
    /// - `token` — token to withdraw.
    /// - `amount` — amount to withdraw.
    ///
    /// # Panics
    /// - `Insufficient non-escrowed funds` if `amount` exceeds the free balance.
    /// - `Amount must be positive` if `amount <= 0`.
    fn emergency_withdraw(e: Env, admin: Address, recipient: Address, token: Address, amount: i128);

    /// Amends a recurring payment's amount and/or interval. The payer authenticates.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `caller` — the payer (authenticated).
    /// - `recurring_id` — recurring payment to amend.
    /// - `new_amount` — new charge amount.
    /// - `new_interval` — new interval in ledgers.
    ///
    /// # Panics
    /// - `amount must be positive` if `new_amount <= 0`.
    /// - `interval must be positive` if `new_interval == 0`.
    /// - `recurring not found` if the record does not exist.
    /// - `not the payer` if the caller is not the payer.
    /// - `recurring is not active` if the schedule is inactive.
    fn amend_recurring(
        e: Env,
        caller: Address,
        recurring_id: u32,
        new_amount: i128,
        new_interval: u32,
    );

    /// Returns the number of active recurring payments owed to `payee`.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `payee` — the payee to query.
    fn recurring_count_for_payee(e: Env, payee: Address) -> u32;

    /// Returns the recurring payment IDs owing to `payee`.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `payee` — the payee to query.
    fn recurring_ids_for_payee(e: Env, payee: Address) -> Vec<u32>;

    /// Cancels a payment split and refunds the pooled funds to the sender.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `caller` — the sender or admin (authenticated).
    /// - `split_id` — split to cancel.
    ///
    /// # Panics
    /// - `split not found` if the split does not exist.
    /// - `already distributed` if the split was already paid out.
    /// - `already cancelled` if the split was already cancelled.
    /// - `not authorised to cancel` if the caller is not the sender or admin.
    fn cancel_split(e: Env, caller: Address, split_id: u32);

    /// Transfers an escrow to a new beneficiary. The depositor authenticates;
    /// the escrow is re-indexed under the new beneficiary.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `depositor` — the depositor (authenticated).
    /// - `escrow_id` — escrow to reassign.
    /// - `new_beneficiary` — the replacement beneficiary.
    ///
    /// # Panics
    /// - `not the depositor` if the caller is not the depositor.
    /// - `escrow not found` if the escrow does not exist.
    fn transfer_escrow_beneficiary(
        e: Env,
        depositor: Address,
        escrow_id: u32,
        new_beneficiary: Address,
    );

    /// Returns the number of tracked token holders.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    fn total_holders(e: Env) -> u32;

    /// Returns the full list of tracked token holder addresses.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    fn get_holders(e: Env) -> Vec<Address>;

    /// Sets the mediation fee charged to the loser of a dispute. Admin-only.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `admin` — the contract admin (authenticated).
    /// - `fee_bps` — mediation fee in basis points.
    fn set_mediation_fee(e: Env, admin: Address, fee_bps: u32);

    /// Returns the contract version string stored at initialization
    /// (defaults to `"1.0.0"`).
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    fn version(e: Env) -> soroban_sdk::String;

    /// Returns contract identity info: version, admin, pause state, and the
    /// initialization ledger.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    ///
    /// # Panics
    /// - `admin not set` if the contract is not initialized.
    fn get_contract_info(e: Env) -> ContractInfo;

    /// Returns a high-level summary: admin, total supply, escrow count, and
    /// total value locked.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    ///
    /// # Panics
    /// - `admin not set` if the contract is not initialized.
    fn contract_summary(e: Env) -> ContractSummary;

    /// Returns the spendable balance of `account` (`0` if frozen).
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `account` — the account to query.
    fn spendable_balance(e: Env, account: Address) -> i128;

    /// Freezes (authorized = false) or unfreezes an account. Admin-only.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `admin` — the contract admin (authenticated).
    /// - `account` — the account to (un)freeze.
    /// - `authorized` — `false` to freeze, `true` to unfreeze.
    fn set_authorized(e: Env, admin: Address, account: Address, authorized: bool);

    /// Adds `amount` to an existing allowance. The owner authenticates.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `from` — the allowance owner (authenticated).
    /// - `spender` — the allowance holder.
    /// - `amount` — amount to add.
    ///
    /// # Panics
    /// - `amount must be positive` if `amount <= 0`.
    fn increase_allowance(e: Env, from: Address, spender: Address, amount: i128);

    /// Removes `amount` from an existing allowance. The owner authenticates.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `from` — the allowance owner (authenticated).
    /// - `spender` — the allowance holder.
    /// - `amount` — amount to remove.
    ///
    /// # Panics
    /// - `amount must be positive` if `amount <= 0`.
    fn decrease_allowance(e: Env, from: Address, spender: Address, amount: i128);

    /// Burns tokens from `from` using an allowance held by `spender`.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `spender` — the allowance holder (authenticated).
    /// - `from` — the account debited.
    /// - `amount` — amount to burn.
    ///
    /// # Panics
    /// - `amount must be positive` if `amount <= 0`.
    /// - `allowance expired` / `insufficient allowance` if the allowance is invalid.
    /// - `insufficient balance` if `from` lacks the funds.
    fn burn_from(e: Env, spender: Address, from: Address, amount: i128);

    /// Transfers tokens with an attached memo (max 64 bytes). The sender
    /// authenticates; whitelist checks apply when enabled.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `from` — the sender (authenticated).
    /// - `to` — the recipient.
    /// - `amount` — transfer amount.
    /// - `memo` — memo tag, up to 64 bytes.
    ///
    /// # Panics
    /// - `amount must be positive` if `amount <= 0`.
    /// - `memo cannot exceed 64 bytes` if the memo is too long.
    /// - `sender not whitelisted` / `recipient not whitelisted` if whitelist mode
    ///   is enabled and a party is not whitelisted.
    fn transfer_with_memo(e: Env, from: Address, to: Address, amount: i128, memo: Bytes);

    /// Revokes every allowance granted by `from`.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `from` — the allowance owner.
    fn revoke_all_allowances(e: Env, from: Address);

    /// Enables whitelist mode (transfers restricted to whitelisted accounts).
    /// Admin-only.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `admin` — the contract admin (authenticated).
    fn enable_whitelist(e: Env, admin: Address);

    /// Disables whitelist mode. Admin-only.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `admin` — the contract admin (authenticated).
    fn disable_whitelist(e: Env, admin: Address);

    /// Whitelists a single account. Admin-only.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `admin` — the contract admin (authenticated).
    /// - `account` — account to whitelist.
    fn add_to_whitelist(e: Env, admin: Address, account: Address);

    /// Removes a single account from the whitelist. Admin-only.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `admin` — the contract admin (authenticated).
    /// - `account` — account to remove.
    fn remove_from_whitelist(e: Env, admin: Address, account: Address);

    /// Returns whether `account` is whitelisted (or whitelist mode is off).
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `account` — the account to check.
    fn is_whitelisted(e: Env, account: Address) -> bool;

    /// Sets the protocol fee (bps, capped at 500 bps) and treasury address.
    /// Admin-only. The fee is applied to escrow releases.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `admin` — the contract admin (authenticated).
    /// - `fee_bps` — protocol fee in basis points (<= 500).
    /// - `treasury` — address receiving collected fees.
    ///
    /// # Panics
    /// - `protocol fee cannot exceed 500 bps` if `fee_bps > 500`.
    fn set_protocol_fee(e: Env, admin: Address, fee_bps: u32, treasury: Address);

    /// Triggers an auto-release of an escrow once its scheduled release ledger
    /// is reached. Permissionless (crank pattern).
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `escrow_id` — escrow to auto-release.
    ///
    /// # Panics
    /// - `auto release not set for this escrow` if no auto-release is scheduled.
    /// - `auto release not yet available` if the release ledger is in the future.
    /// - `escrow already settled` if the escrow is released or refunded.
    fn trigger_auto_release(e: Env, escrow_id: u32);

    /// Returns the shared escrow ID between two addresses (either direction).
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `addr1` — first address.
    /// - `addr2` — second address.
    ///
    /// # Panics
    /// - `no escrow found between the two addresses` if no shared escrow exists.
    fn escrow_between(e: Env, addr1: Address, addr2: Address) -> u32;

    fn get_all_escrows_between(e: Env, depositor: Address, beneficiary: Address) -> Vec<u32>;

    /// Cancels up to 20 recurring payments in one call. The payer authenticates.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `caller` — the payer (authenticated).
    /// - `recurring_ids` — recurring payment IDs to cancel.
    ///
    /// # Panics
    /// - `batch size cannot exceed 20` if the batch is too large.
    /// - `recurring not found` / `not the payer for recurring {id}` /
    ///   `recurring {id} is not active` for any failing entry.
    fn cancel_recurring_batch(e: Env, caller: Address, recurring_ids: Vec<u32>);

    /// Adds `amount` to an existing escrow. The depositor authenticates.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `depositor` — the depositor (authenticated).
    /// - `escrow_id` — escrow to top up.
    /// - `amount` — amount to add.
    ///
    /// # Panics
    /// - `amount must be positive` if `amount <= 0`.
    /// - `DisputeOpen: cannot top up an escrow under active dispute` if disputed.
    /// - `escrow already settled` if released or refunded.
    /// - `not the depositor` if the caller is not the depositor.
    fn topup_escrow(e: Env, depositor: Address, escrow_id: u32, amount: i128);

    /// Creates a vesting schedule: `holder` receives `amount` tokens after
    /// `vesting_ledger`. Funds are locked in the contract until then. Admin-only.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `admin` — the contract admin (authenticated).
    /// - `holder` — the beneficiary of the vesting schedule.
    /// - `token` — token being locked.
    /// - `amount` — amount locked.
    /// - `vesting_ledger` — ledger when the tokens become claimable.
    ///
    /// # Panics
    /// - `vesting ledger must be in the future` if the ledger is not in the future.
    /// - `Amount must be strictly positive` if `amount <= 0`.
    fn create_vesting(e: Env, admin: Address, holder: Address, token: Address, amount: i128, vesting_ledger: u32) -> u32;

    /// Claims a matured vesting schedule. The holder authenticates.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `holder` — the vesting holder (authenticated).
    /// - `vesting_id` — vesting schedule to claim.
    ///
    /// # Panics
    /// - `vesting record not found` if the schedule does not exist.
    /// - `not the vesting holder` if the caller is not the holder.
    /// - `vesting already claimed` if already claimed.
    /// - `vesting period not yet reached` if the ledger is too early.
    fn claim_vesting(e: Env, holder: Address, vesting_id: u32);

    /// Returns all vesting IDs assigned to a holder.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `holder` — the holder to query.
    fn get_vesting_by_holder(e: Env, holder: Address) -> Vec<u32>;

    /// Splits a payment into multiple escrows, one per recipient, weighted by
    /// basis points. The final recipient receives any rounding dust.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `sender` — payer funding the split (authenticated).
    /// - `recipients` — list of `(address, share_bps)` where shares sum to 10000.
    /// - `token` — token being split.
    /// - `total_amount` — total amount to split.
    /// - `expiry_ledger` — expiry ledger for each created escrow.
    ///
    /// # Panics
    /// - `Amount must be strictly positive` if `total_amount <= 0`.
    /// - `recipient share_bps cannot be zero` if any share is zero.
    /// - `duplicate recipient address` if the list contains duplicates.
    /// - `total basis points must equal 10000` if shares do not sum to 10000.
    /// - `split remaining underflow` on arithmetic underflow.
    fn split_to_escrow(
        e: Env,
        sender: Address,
        recipients: Vec<(Address, u32)>,
        token: Address,
        total_amount: i128,
        expiry_ledger: u32,
    ) -> Vec<u32>;

    /// Distributes `total_amount` of `token` pro-rata to tracked holders,
    /// skipping frozen accounts, with the remainder returned to the admin.
    /// Admin-only.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `admin` — the contract admin (authenticated).
    /// - `token` — token being airdropped.
    /// - `total_amount` — total amount to distribute.
    ///
    /// # Panics
    /// - `Amount must be strictly positive` if `total_amount <= 0`.
    /// - `no holders to airdrop to` if the holder set is empty.
    /// - `insufficient admin balance` if `admin` lacks the funds.
    /// - `no eligible holders with positive balance` if no holder is eligible.
    /// - `airdrop distributed overflow` / `airdrop remainder underflow` on
    ///   arithmetic failure.
    fn airdrop(e: Env, admin: Address, token: Address, total_amount: i128);

    /// Sets multiple allowances from a single signed message. The signature is
    /// verified against `public_key` and the nonce is consumed once.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `owner` — the allowance owner.
    /// - `approvals` — list of `(spender, amount, expiration_ledger)`.
    /// - `nonce` — current nonce for the owner.
    /// - `public_key` — ed25519 public key (32 bytes).
    /// - `signature` — ed25519 signature (64 bytes).
    ///
    /// # Panics
    /// - `approvals cannot be empty` if the list is empty.
    /// - `TooManyApprovals: maximum 20 approvals per batch` if the list is too long.
    /// - `invalid public_key byte length` if the key is malformed.
    /// - `invalid nonce` if the nonce is stale.
    fn permit_batch(
        e: Env,
        owner: Address,
        approvals: Vec<(Address, i128, u32)>,
        nonce: u64,
        public_key: BytesN<32>,
        signature: BytesN<64>,
    );

    /// Replaces a recipient in an undistributed split, preserving the share.
    /// The split sender authenticates.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `sender` — the split sender (authenticated).
    /// - `split_id` — split to update.
    /// - `old_recipient` — recipient being replaced.
    /// - `new_recipient` — replacement recipient.
    ///
    /// # Panics
    /// - `split not found` if the split does not exist.
    /// - `not authorised to replace recipient` if the caller is not the sender.
    /// - `split has already been distributed` / `split has been cancelled`.
    /// - `old recipient not found in split`.
    /// - `new recipient is already in the split`.
    fn replace_split_recipient(
        e: Env,
        sender: Address,
        split_id: u32,
        old_recipient: Address,
        new_recipient: Address,
    );

    /// Approves multiple allowances in one call. The owner authenticates.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `from` — the allowance owner (authenticated).
    /// - `approvals` — list of `(spender, amount, expiration_ledger)`.
    fn approve_batch(e: Env, from: Address, approvals: Vec<(Address, i128, u32)>);

    /// Clawbacks tokens from multiple accounts in one call. Admin-only.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `admin` — the contract admin (authenticated).
    /// - `clawbacks` — list of `(from, amount)`.
    fn clawback_batch(e: Env, admin: Address, clawbacks: Vec<(Address, i128)>);

    /// Mints tokens to multiple accounts in one call, returning the total
    /// minted. Admin-only.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `admin` — the contract admin (authenticated).
    /// - `mints` — list of `(to, amount)`.
    fn mint_batch(e: Env, admin: Address, mints: Vec<(Address, i128)>) -> i128;

    /// Cancels a single recurring payment. The payer authenticates.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `caller` — the payer (authenticated).
    /// - `recurring_id` — recurring payment to cancel.
    ///
    /// # Panics
    /// - `recurring not found` if the record does not exist.
    /// - `not the payer` if the caller is not the payer.
    /// - `recurring is not active` if the schedule is already inactive.
    fn cancel_recurring(e: Env, caller: Address, recurring_id: u32);

    /// Returns the recurring payment IDs funded by `payer`.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `payer` — the payer to query.
    fn get_recurring_by_payer(e: Env, payer: Address) -> Vec<u32>;

    /// Takes a balance snapshot of `account`. Admin-only.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `admin` — the contract admin (authenticated).
    /// - `account` — account to snapshot.
    fn take_snapshot(e: Env, admin: Address, account: Address);

    /// Returns the snapshot balance captured for `account` (or `0`).
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `account` — the account to query.
    fn get_snapshot_balance(e: Env, account: Address) -> i128;

    /// Returns the ledger at which `account`'s snapshot was taken (or `0`).
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `account` — the account to query.
    fn snapshot_taken_at(e: Env, account: Address) -> u32;

    /// Admin-settles an escrow to `winner`, overriding the normal
    /// release/refund path (e.g. on deadlock). Admin-only.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `admin` — the contract admin (authenticated).
    /// - `escrow_id` — escrow to settle.
    /// - `winner` — the party that receives the funds.
    ///
    /// # Panics
    /// - `escrow already settled` if the escrow is released or refunded.
    /// - `winner must be depositor or beneficiary` if the winner is not a party.
    /// - `nothing left to settle` if nothing remains.
    fn admin_settle_escrow(e: Env, admin: Address, escrow_id: u32, winner: Address);

    /// Appeals an open dispute within the appeal window. Only a party can appeal.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `caller` — the depositor or beneficiary (authenticated).
    /// - `escrow_id` — escrow under dispute.
    ///
    /// # Panics
    /// - `escrow already settled`, `escrow is not under dispute`.
    /// - `only depositor or beneficiary can appeal` if the caller is not a party.
    /// - `dispute already has a pending appeal`.
    /// - `appeal window has expired` if the window has closed.
    fn appeal_dispute(e: Env, caller: Address, escrow_id: u32);

    /// Resolves a pending dispute appeal, paying the declared winner. The
    /// arbiter authenticates.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `resolver` — the arbiter (authenticated).
    /// - `escrow_id` — escrow under appeal.
    /// - `winner` — the party awarded the funds.
    ///
    /// # Panics
    /// - `Unauthorized: only the arbiter can resolve disputes`.
    /// - `no pending appeal to resolve`, `escrow is not under dispute`,
    ///   `escrow already settled`, `winner must be depositor or beneficiary`.
    fn resolve_appeal(e: Env, resolver: Address, escrow_id: u32, winner: Address);

    /// Expires a dispute that has been open too long without resolution,
    /// unfreezing the escrow. Only a party can expire.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `caller` — the depositor or beneficiary (authenticated).
    /// - `escrow_id` — escrow whose dispute is expired.
    ///
    /// # Panics
    /// - `escrow already settled`, `escrow is not under dispute`.
    /// - `only depositor or beneficiary can expire a dispute`.
    /// - `dispute has not been open long enough to expire`.
    fn expire_dispute(e: Env, caller: Address, escrow_id: u32);

    /// Pauses a recurring payment. Only the payer can pause.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `caller` — the payer (authenticated).
    /// - `recurring_id` — recurring payment to pause.
    ///
    /// # Panics
    /// - `recurring not found` if the record does not exist.
    /// - `unauthorized: only the payer can pause a recurring payment` if the
    ///   caller is not the payer.
    /// - `recurring is not active` if already inactive.
    fn pause_recurring(e: Env, caller: Address, recurring_id: u32);

    /// Resumes a paused recurring payment. Only the payer can resume.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `caller` — the payer (authenticated).
    /// - `recurring_id` — recurring payment to resume.
    ///
    /// # Panics
    /// - `recurring not found` if the record does not exist.
    /// - `unauthorized: only the payer can resume a recurring payment` if the
    ///   caller is not the payer.
    /// - `recurring is already active` if not paused.
    fn resume_recurring(e: Env, caller: Address, recurring_id: u32);

    /// Whitelists up to 50 accounts in a single admin call.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `admin` — the contract admin (authenticated).
    /// - `accounts` — list of accounts to whitelist (max 50).
    ///
    /// # Panics
    /// - `TooManyAccounts: maximum 50 accounts per batch` if the list is too long.
    // #735: transfer a recurring payment to a new payer
    fn transfer_recurring_payer(e: Env, caller: Address, recurring_id: u32, new_payer: Address);
    // #749: recurring execution window
    fn set_recurring_execution_window(e: Env, admin: Address, window_ledgers: u32);
    // #743: timed freeze
    fn freeze_until(e: Env, admin: Address, account: Address, until_ledger: u32);
    // #748: signed bulk whitelist
    fn add_to_whitelist_signed(
        e: Env,
        admin: Address,
        addresses: Vec<Address>,
        nonce: u64,
        public_key: BytesN<32>,
        signature: BytesN<64>,
    );
    // #741: batch whitelist add (max 50 accounts)
    fn add_to_whitelist_batch(e: Env, admin: Address, accounts: Vec<Address>);
}

#[contracttype]
#[derive(Clone)]
pub struct ContractSummary {
    pub admin: Address,
    pub total_supply: i128,
    pub escrow_count: u32,
    pub total_value_locked: i128,
}

#[contract]
pub struct VeriTixPay;

#[contractimpl]
impl VeriTixPayTrait for VeriTixPay {
    fn initialize(env: Env, admin: Address) {
        admin::validate_admin_address(&env, &admin);

        if env.storage().persistent().has(&DataKey::Admin) {
            panic!("AlreadyInitialized: contract state is locked");
        }

        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::InitializedAtLedger, &env.ledger().sequence());
    }

    fn initialize_with_max_supply(env: Env, admin: Address, max_supply: i128) {
        admin::validate_admin_address(&env, &admin);

        if env.storage().persistent().has(&DataKey::Admin) {
            panic!("AlreadyInitialized: contract state is locked");
        }

        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::MaxSupply, &max_supply);
        env.storage().persistent().set(&DataKey::MaxSupply, &max_supply);
        env.storage()
            .persistent()
            .set(&DataKey::InitializedAtLedger, &env.ledger().sequence());
    }

    // ── SEP-41 Token Interface ────────────────────────────────────────────────

    fn name(e: Env) -> soroban_sdk::String {
        soroban_sdk::String::from_str(&e, "VeriTix")
    }

    fn symbol(e: Env) -> soroban_sdk::String {
        soroban_sdk::String::from_str(&e, "VTX")
    }

    fn decimals(_e: Env) -> u32 {
        7
    }

    fn balance(e: Env, account: Address) -> i128 {
        crate::balance::balance_of(&e, &account)
    }

    fn total_supply(e: Env) -> i128 {
        crate::balance::read_supply(&e)
    }

    fn mint(e: Env, admin: Address, to: Address, amount: i128) {
        admin::check_admin(&e, &admin);
        require_positive_amount(amount);
        crate::balance::increase_supply(&e, amount);
        crate::balance::add_balance(&e, &to, amount);
    }

    fn burn(e: Env, from: Address, amount: i128) {
        from.require_auth();
        require_positive_amount(amount);
        let bal = crate::balance::balance_of(&e, &from);
        assert!(bal >= amount, "insufficient balance");
        crate::balance::decrease_supply(&e, amount);
        let new_balance = bal - amount;
        if new_balance == 0 {
            e.storage()
                .persistent()
                .remove(&DataKey::BalanceOf(from.clone()));
        } else {
            e.storage()
                .persistent()
                .set(&DataKey::BalanceOf(from.clone()), &new_balance);
        }
    }

    fn clawback(e: Env, admin: Address, from: Address, amount: i128) {
        admin::check_admin(&e, &admin);
        require_positive_amount(amount);
        let bal = crate::balance::balance_of(&e, &from);
        assert!(bal >= amount, "insufficient balance");
        crate::balance::decrease_supply(&e, amount);
        let new_balance = bal - amount;
        if new_balance == 0 {
            e.storage()
                .persistent()
                .remove(&DataKey::BalanceOf(from.clone()));
        } else {
            e.storage()
                .persistent()
                .set(&DataKey::BalanceOf(from.clone()), &new_balance);
        }
    }

    fn create_escrow(
        e: Env,
        depositor: Address,
        beneficiary: Address,
        token: Address,
        amount: i128,
        expiry_ledger: u32,
        memo: Bytes,
    ) -> u32 {
        require_positive_amount(amount);
        escrow::create_escrow(
            e,
            depositor,
            beneficiary,
            token,
            amount,
            expiry_ledger,
            memo,
        )
    }

    fn release_escrow(e: Env, caller: Address, escrow_id: u32) {
        // Track the beneficiary in the holder set for airdrop (#573)
        let record = escrow::load_record(&e, escrow_id);
        let beneficiary = record.beneficiary.clone();
        escrow::release_escrow(e.clone(), caller, escrow_id);
        track_holder_for_airdrop(&e, &beneficiary);
    }

    fn release_partial_escrow(e: Env, caller: Address, escrow_id: u32, amount: i128) {
        require_positive_amount(amount);
        // Track the beneficiary in the holder set for airdrop (#573)
        let record = escrow::load_record(&e, escrow_id);
        let beneficiary = record.beneficiary.clone();
        escrow::release_partial_escrow(e.clone(), caller, escrow_id, amount);
        track_holder_for_airdrop(&e, &beneficiary);
    }

    fn refund_escrow(e: Env, caller: Address, escrow_id: u32) {
        escrow::refund_escrow(e, caller, escrow_id)
    }

    fn get_escrows_by_depositor(e: Env, depositor: Address) -> Vec<u32> {
        escrow::get_escrows_by_depositor(e, depositor)
    }

    fn get_escrows_by_beneficiary(e: Env, beneficiary: Address) -> Vec<u32> {
        escrow::get_escrows_by_beneficiary(e, beneficiary)
    }

    fn escrowed_total(e: Env) -> i128 {
        escrow::get_escrowed_total(&e)
    }

    fn escrow_stats(e: Env) -> escrow::EscrowStats {
        escrow::get_escrow_stats(&e)
    }

    fn place_lien(e: Env, creditor: Address, escrow_id: u32, lien_amount: i128) {
        escrow::place_lien(e, creditor, escrow_id, lien_amount)
    }

    fn clear_lien(e: Env, caller: Address, escrow_id: u32) {
        escrow::clear_lien(e, caller, escrow_id)
    }

    fn get_escrow(e: Env, escrow_id: u32) -> escrow::EscrowRecord {
        escrow::load_record(&e, escrow_id)
    }

    fn is_escrow_settled(e: Env, escrow_id: u32) -> bool {
        match e
            .storage()
            .persistent()
            .get::<DataKey, escrow::EscrowRecord>(&DataKey::Escrow(escrow_id))
        {
            Some(escrow) => escrow.released || escrow.refunded,
            None => true,
        }
    }

    fn get_disputes_by_claimant(e: Env, claimant: Address) -> Vec<u32> {
        dispute::get_disputes_by_claimant(e, claimant)
    }

    fn set_arbiter(e: Env, arbiter: Address) {
        dispute::set_arbiter(&e, &arbiter)
    }

    fn resolve_dispute(e: Env, resolver: Address, escrow_id: u32, winner: Address) {
        dispute::resolve_dispute(&e, &resolver, escrow_id, &winner)
    }

    fn is_dispute_open(e: Env, escrow_id: u32) -> bool {
        e.storage()
            .persistent()
            .has(&DataKey::EscrowDispute(escrow_id))
    }

    fn setup_recurring(
        e: Env,
        payer: Address,
        payee: Address,
        token: Address,
        amount: i128,
        interval: u32,
        max_executions: u32,
    ) -> u32 {
        recurring::setup_recurring(&e, payer, payee, token, amount, interval, max_executions)
    }

    fn execute_recurring(e: Env, recurring_id: u32) {
        recurring::execute_recurring(&e, recurring_id)
    }

    fn get_recurring_history(e: Env, recurring_id: u32) -> Vec<RecurringPayment> {
        recurring::get_recurring_history(e, recurring_id)
    }

    fn is_recurring_active(e: Env, recurring_id: u32) -> bool {
        match e
            .storage()
            .persistent()
            .get::<DataKey, recurring::RecurringRecord>(&DataKey::Recurring(recurring_id))
        {
            Some(record) => record.active,
            None => false,
        }
    }

    fn get_escrows_batch(e: Env, escrow_ids: Vec<u32>) -> Vec<Option<escrow::EscrowRecord>> {
        escrow::get_escrows_batch(e, escrow_ids)
    }

    fn get_escrow_age(e: Env, escrow_id: u32) -> u32 {
        escrow::get_escrow_age(e, escrow_id)
    }

    fn create_multi_escrow(
        e: Env,
        depositor: Address,
        recipients: Vec<(Address, i128)>,
        token: Address,
        expiry_ledger: u32,
    ) -> u32 {
        // Enforce that total distributed amount values are checked within sub-module contexts
        multi_escrow::create_multi_escrow(e, depositor, recipients, token, expiry_ledger)
    }

    fn release_multi_escrow(e: Env, caller: Address, multi_escrow_id: u32) {
        multi_escrow::release_multi_escrow(e, caller, multi_escrow_id)
    }

    fn refund_multi_escrow(e: Env, caller: Address, multi_escrow_id: u32) {
        multi_escrow::refund_multi_escrow(e, caller, multi_escrow_id)
    }

    fn ticket_escrow(
        e: Env,
        buyer: Address,
        organizer: Address,
        token: Address,
        ticket_price: i128,
        event_ledger: u32,
        ticket_ref: Bytes,
    ) -> u32 {
        buyer.require_auth();
        require_positive_amount(ticket_price);

        escrow::create_escrow(
            e,
            buyer,
            organizer,
            token,
            ticket_price,
            event_ledger + 100,
            ticket_ref,
        )
    }

    fn revenue_split(
        e: Env,
        sender: Address,
        organizer: Address,
        organizer_bps: u32,
        artist: Address,
        artist_bps: u32,
        platform: Address,
        token: Address,
        total_amount: i128,
        event_ledger: u32,
    ) -> u32 {
        sender.require_auth();
        require_positive_amount(total_amount);

        assert!(organizer_bps + artist_bps < 10_000, "invalid basis points");
        let _platform_bps = 10_000 - organizer_bps - artist_bps;
        let organizer_amt = total_amount * organizer_bps as i128 / 10_000;
        let artist_amt = total_amount * artist_bps as i128 / 10_000;
        let platform_amt = total_amount - organizer_amt - artist_amt;

        let recipients = Vec::from_array(
            &e,
            [
                (organizer, organizer_amt),
                (artist, artist_amt),
                (platform, platform_amt),
            ],
        );
        let split_id = multi_escrow::create_multi_escrow(
            e.clone(),
            sender.clone(),
            recipients,
            token,
            event_ledger + 100,
        );
        multi_escrow::release_multi_escrow(e, sender, split_id);
        split_id
    }

    fn approve(e: Env, from: Address, spender: Address, amount: i128, expiration_ledger: u32) {
        from.require_auth();
        require_positive_amount(amount);
        allowance::create_allowance(&e, &from, &spender, amount, expiration_ledger);
    }

    fn transfer_from(e: Env, spender: Address, from: Address, to: Address, amount: i128) {
        crate::pause::require_not_paused(&e);
        spender.require_auth();
        require_positive_amount(amount);
        allowance::spend_allowance(&e, &from, &spender, amount);
        crate::balance::spend_balance(&e, &from, amount);
        crate::balance::add_balance(&e, &to, amount);
    }

    // ── #451: Admin ownership ────────────────────────────────────────────────

    fn transfer_ownership(e: Env, new_admin: Address) {
        admin::transfer_ownership(&e, &new_admin)
    }

    fn accept_admin(e: Env, new_admin: Address) {
        admin::accept_admin(&e, &new_admin)
    }

    fn admin_active_after_ledger(e: Env) -> u32 {
        admin::admin_active_after_ledger(&e)
    }

    // ── #452: Depositor escrowed value ───────────────────────────────────────

    fn escrowed_value_for_depositor(e: Env, depositor: Address) -> i128 {
        escrow::escrowed_value_for_depositor(&e, &depositor)
    }

    // ── Pause ─────────────────────────────────────────────────────────────────

    fn set_paused(e: Env, admin: Address, paused: bool) {
        crate::pause::set_paused(&e, &admin, paused);
    }

    fn is_paused(e: Env) -> bool {
        e.storage()
            .persistent()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
    }

    fn contract_paused_for(e: Env) -> Option<u32> {
        crate::pause::contract_paused_for(&e)
    }

    // ── Permit / Nonce ────────────────────────────────────────────────────────

    fn permit(e: Env, user: Address, nonce: u32) {
        user.require_auth();
        crate::permit::check_and_increment_nonce(&e, &user, nonce);
    }

    fn nonces(e: Env, user: Address) -> u64 {
        permit::nonces(&e, user)
    }

    // ── #453: Resolver stats ─────────────────────────────────────────────────

    fn resolver_stats(e: Env, resolver: Address) -> ResolverStats {
        dispute::get_resolver_stats(&e, &resolver)
    }

    // ── #454: Protocol fee stats ─────────────────────────────────────────────

    fn protocol_fee_stats(e: Env) -> (u32, Address, i128) {
        let fee_bps: u32 = e.storage().persistent().get(&DataKey::FeeBps).unwrap_or(0);
        let treasury: Address = e
            .storage()
            .persistent()
            .get(&DataKey::TreasuryAddress)
            .unwrap_or_else(|| {
                e.storage()
                    .persistent()
                    .get(&DataKey::Admin)
                    .expect("admin not set")
            });
        let total_collected: i128 = e
            .storage()
            .persistent()
            .get(&DataKey::TotalFeesCollected)
            .unwrap_or(0);
        (fee_bps, treasury, total_collected)
    }

    fn emergency_withdraw(
        e: Env,
        admin: Address,
        recipient: Address,
        token: Address,
        amount: i128,
    ) {
        // Verify caller is admin and authenticated
        admin::check_admin(&e, &admin);

        // Get contract's current balance of the token
        let token_client = soroban_sdk::token::Client::new(&e, &token);
        let contract_balance = token_client.balance(&e.current_contract_address());

        // Get total escrowed value (locked funds that cannot be touched)
        let total_escrowed = escrow::get_escrowed_total(&e);

        // Verify we're not withdrawing more than the stranded funds (contract balance - escrowed funds)
        assert!(
            amount <= contract_balance - total_escrowed,
            "Insufficient non-escrowed funds"
        );
        assert!(amount > 0, "Amount must be positive");

        // Transfer the amount from the contract to the recipient
        token_client.transfer(&e.current_contract_address(), &recipient, &amount);

        // Emit the emergency withdrawal event
        e.events().publish(
            (soroban_sdk::symbol_short!("em_wdraw"), admin, recipient),
            amount,
        );
    }

    fn amend_recurring(
        e: Env,
        caller: Address,
        recurring_id: u32,
        new_amount: i128,
        new_interval: u32,
    ) {
        recurring::amend_recurring(&e, &caller, recurring_id, new_amount, new_interval)
    }

    fn escrow_between(e: Env, addr1: Address, addr2: Address) -> u32 {
        escrow::escrow_between(e, addr1, addr2)
    }

    fn get_all_escrows_between(e: Env, depositor: Address, beneficiary: Address) -> Vec<u32> {
        escrow::get_all_escrows_between(e, depositor, beneficiary)
    }

    fn cancel_recurring_batch(e: Env, caller: Address, recurring_ids: Vec<u32>) {
        recurring::cancel_recurring_batch(&e, &caller, recurring_ids)
    }

    fn cancel_split(e: Env, caller: Address, split_id: u32) {
        crate::splitter::cancel_split(e, caller, split_id)
    }

    fn replace_split_recipient(
        e: Env,
        sender: Address,
        split_id: u32,
        old_recipient: Address,
        new_recipient: Address,
    ) {
        crate::splitter::replace_split_recipient(e, sender, split_id, old_recipient, new_recipient)
    }

    fn approve_batch(e: Env, from: Address, approvals: Vec<(Address, i128, u32)>) {
        crate::batch::approve_batch(&e, from, approvals);
    }

    fn clawback_batch(e: Env, admin: Address, clawbacks: Vec<(Address, i128)>) {
        crate::batch::clawback_batch(&e, admin, clawbacks);
    }

    fn mint_batch(e: Env, admin: Address, mints: Vec<(Address, i128)>) -> i128 {
        crate::batch::mint_batch(&e, admin, mints)
    }

    fn cancel_recurring(e: Env, caller: Address, recurring_id: u32) {
        recurring::cancel_recurring(&e, &caller, recurring_id);
    }

    fn get_recurring_by_payer(e: Env, payer: Address) -> Vec<u32> {
        recurring::get_recurring_by_payer(&e, &payer)
    }

    fn take_snapshot(e: Env, admin: Address, account: Address) {
        snapshot::take_snapshot(&e, &admin, &account)
    }

    fn get_snapshot_balance(e: Env, account: Address) -> i128 {
        snapshot::get_snapshot_balance(&e, &account)
    }

    fn snapshot_taken_at(e: Env, account: Address) -> u32 {
        snapshot::snapshot_taken_at(&e, &account)
    }

    fn admin_settle_escrow(e: Env, admin: Address, escrow_id: u32, winner: Address) {
        escrow::admin_settle_escrow(e, admin, escrow_id, winner)
    }

    fn appeal_dispute(e: Env, caller: Address, escrow_id: u32) {
        dispute::appeal_dispute(&e, &caller, escrow_id)
    }

    fn resolve_appeal(e: Env, resolver: Address, escrow_id: u32, winner: Address) {
        dispute::resolve_appeal(&e, &resolver, escrow_id, &winner)
    }

    fn expire_dispute(e: Env, caller: Address, escrow_id: u32) {
        dispute::expire_dispute(&e, &caller, escrow_id)
    }

    fn pause_recurring(e: Env, caller: Address, recurring_id: u32) {
        recurring::pause_recurring(&e, &caller, recurring_id)
    }

    fn resume_recurring(e: Env, caller: Address, recurring_id: u32) {
        recurring::resume_recurring(&e, &caller, recurring_id)
    }

    fn topup_escrow(e: Env, depositor: Address, escrow_id: u32, amount: i128) {
        escrow::topup_escrow(e, depositor, escrow_id, amount)
    }

    fn create_vesting(e: Env, admin: Address, holder: Address, token: Address, amount: i128, vesting_ledger: u32) -> u32 {
        admin::check_admin(&e, &admin);
        require_positive_amount(amount);
        assert!(vesting_ledger > e.ledger().sequence(), "vesting ledger must be in the future");

        // Lock the deposited tokens into the contract until the vesting date.
        let token_client = soroban_sdk::token::Client::new(&e, &token);
        token_client.transfer(&admin, &e.current_contract_address(), &amount);

        let id: u32 = e.storage().persistent().get(&DataKey::VestingCount).unwrap_or(0) + 1;
        e.storage().persistent().set(&DataKey::VestingCount, &id);

        let record = VestingRecord {
            id,
            holder: holder.clone(),
            token,
            amount,
            vesting_ledger,
            claimed: false,
        };
        e.storage().persistent().set(&DataKey::Vesting(id), &record);

        let mut holder_vestings: Vec<u32> = e.storage()
            .persistent()
            .get(&DataKey::HolderVestings(holder.clone()))
            .unwrap_or_else(|| Vec::new(&e));
        holder_vestings.push_back(id);
        e.storage().persistent().set(&DataKey::HolderVestings(holder), &holder_vestings);

        id
    }

    fn claim_vesting(e: Env, holder: Address, vesting_id: u32) {
        holder.require_auth();

        let mut record: VestingRecord = e
            .storage()
            .persistent()
            .get(&DataKey::Vesting(vesting_id))
            .expect("vesting record not found");

        assert!(record.holder == holder, "not the vesting holder");
        assert!(!record.claimed, "vesting already claimed");
        assert!(
            e.ledger().sequence() >= record.vesting_ledger,
            "vesting period not yet reached"
        );

        let token_client = soroban_sdk::token::Client::new(&e, &record.token);
        token_client.transfer(&e.current_contract_address(), &holder, &record.amount);

        record.claimed = true;
        e.storage()
            .persistent()
            .set(&DataKey::Vesting(vesting_id), &record);
    }

    fn raise_dispute(e: Env, caller: Address, escrow_id: u32) {
        let max_disputes: u32 = e
            .storage()
            .persistent()
            .get(&DataKey::MaxDisputes)
            .unwrap_or(3);
        let current_count: u32 = e
            .storage()
            .persistent()
            .get(&DataKey::DisputeCount(escrow_id))
            .unwrap_or(0);
        assert!(
            current_count < max_disputes,
            "maximum dispute count exceeded"
        );
        e.storage()
            .persistent()
            .set(&DataKey::DisputeCount(escrow_id), &(current_count + 1));
        dispute::raise_dispute(&e, &caller, escrow_id)
    }

    fn version(e: Env) -> soroban_sdk::String {
        e.storage()
            .persistent()
            .get(&DataKey::Version)
            .unwrap_or(String::from_str(&e, "1.0.0"))
    }

    fn get_contract_info(e: Env) -> ContractInfo {
        let version: soroban_sdk::String =
            e.storage().persistent().get(&DataKey::Version).unwrap_or(String::from_str(&e, "1.0.0"));
        let admin: Address = e.storage().persistent().get(&DataKey::Admin).expect("admin not set");
        let is_paused: bool =
            e.storage().persistent().get(&DataKey::Paused).unwrap_or(false);
        let initialized_at_ledger: u32 =
            e.storage().persistent().get(&DataKey::InitializedAtLedger).unwrap_or(0);
        ContractInfo { version, admin, is_paused, initialized_at_ledger }
    }

    fn dispute_stats(e: Env) -> DisputeStats {
        crate::dispute::get_dispute_stats(&e)
    }

    fn full_token_info(e: Env) -> FullTokenInfo {
        let version: soroban_sdk::String =
            e.storage().persistent().get(&DataKey::Version).unwrap_or(String::from_str(&e, "1.0.0"));
        let max_supply: i128 = e.storage().persistent().get(&DataKey::MaxSupply).unwrap_or(i128::MAX);
        FullTokenInfo {
            name: soroban_sdk::String::from_str(&e, "VeriTix"),
            symbol: soroban_sdk::String::from_str(&e, "VTX"),
            decimal: 7,
            total_supply: balance::read_supply(&e),
            max_supply,
            version,
        }
    }

    fn escrow_stats_for_depositor(e: Env, depositor: Address) -> EscrowDepositorStats {
        crate::escrow::escrow_stats_for_depositor(&e, &depositor)
    }

    fn contract_summary(e: Env) -> ContractSummary {
        let admin: Address = e
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("admin not set");
        let total_supply: i128 = e
            .storage()
            .persistent()
            .get(&DataKey::TotalSupply)
            .unwrap_or(0);
        let escrow_count: u32 = e
            .storage()
            .persistent()
            .get(&DataKey::EscrowCount)
            .unwrap_or(0);
        let total_value_locked: i128 = e
            .storage()
            .persistent()
            .get(&DataKey::EscrowValueLocked)
            .unwrap_or(0);
        ContractSummary {
            admin,
            total_supply,
            escrow_count,
            total_value_locked,
        }
    }

    fn spendable_balance(e: Env, account: Address) -> i128 {
        balance::spendable_balance(&e, &account)
    }

    fn get_vesting_by_holder(e: Env, holder: Address) -> Vec<u32> {
        e.storage()
            .persistent()
            .get(&DataKey::HolderVestings(holder))
            .unwrap_or_else(|| Vec::new(&e))
    }

    fn burn_from(e: Env, spender: Address, from: Address, amount: i128) {
        balance::burn_from(&e, &spender, &from, amount)
    }

    fn transfer_with_memo(e: Env, from: Address, to: Address, amount: i128, memo: Bytes) {
        from.require_auth();
        assert!(amount > 0, "amount must be positive");
        assert!(memo.len() <= 64, "memo cannot exceed 64 bytes");
        whitelist::check(&e, &from, &to);
        let token_client = soroban_sdk::token::Client::new(&e, &e.current_contract_address());
        token_client.transfer(&from, &to, &amount);
        e.events().publish(
            (soroban_sdk::symbol_short!("transfer"), from, to),
            (amount, memo),
        );
    }

    // ── #572: Split-to-escrow ────────────────────────────────────────────────

    fn split_to_escrow(
        e: Env,
        sender: Address,
        recipients: Vec<(Address, u32)>,
        token: Address,
        total_amount: i128,
        expiry_ledger: u32,
    ) -> Vec<u32> {
        sender.require_auth();
        require_positive_amount(total_amount);

        // Validate BPS sum equals 10000 and check for duplicates
        let mut total_bps: u32 = 0;
        for i in 0..recipients.len() {
            let (addr, bps) = recipients.get(i).unwrap();
            assert!(bps > 0, "recipient share_bps cannot be zero");
            total_bps += bps;
            for j in (i + 1)..recipients.len() {
                let (other_addr, _) = recipients.get(j).unwrap();
                assert!(addr != other_addr, "duplicate recipient address");
            }
        }
        assert!(total_bps == 10000, "total basis points must equal 10000");

        let token_client = token::Client::new(&e, &token);
        token_client.transfer(&sender, &e.current_contract_address(), &total_amount);

        let mut escrow_ids = Vec::new(&e);
        let mut remaining = total_amount;
        let len = recipients.len();
        let empty_memo = Bytes::new(&e);

        for i in 0..len {
            let (address, bps) = recipients.get(i).unwrap();
            let share = if i == len - 1 {
                remaining
            } else {
                total_amount * bps as i128 / 10000
            };
            remaining = remaining
                .checked_sub(share)
                .expect("split remaining underflow");

            let escrow_id = escrow::create_escrow_batch(
                &e,
                &sender,
                &address,
                &token,
                share,
                expiry_ledger,
                &empty_memo,
            );
            escrow_ids.push_back(escrow_id);
        }

        e.events().publish(
            (soroban_sdk::symbol_short!("split_esc"), sender),
            total_amount,
        );

        escrow_ids
    }

    // ── #573: Airdrop ────────────────────────────────────────────────────────

    fn airdrop(e: Env, admin: Address, token: Address, total_amount: i128) {
        admin::check_admin(&e, &admin);
        require_positive_amount(total_amount);

        let holder_count: u32 = e
            .storage()
            .persistent()
            .get(&DataKey::HolderCount)
            .unwrap_or(0);
        assert!(holder_count > 0, "no holders to airdrop to");

        let token_client = token::Client::new(&e, &token);
        let admin_balance = token_client.balance(&admin);
        assert!(admin_balance >= total_amount, "insufficient admin balance");

        token_client.transfer(&admin, &e.current_contract_address(), &total_amount);

        let holders: Vec<Address> = e
            .storage()
            .persistent()
            .get(&DataKey::HolderSet)
            .unwrap_or_else(|| Vec::new(&e));

        let mut total_held: i128 = 0;
        for i in 0..holders.len() {
            let holder = holders.get(i).unwrap();
            let frozen: bool = e
                .storage()
                .persistent()
                .get(&DataKey::Frozen(holder.clone()))
                .unwrap_or(false);
            if !frozen {
                let balance = token_client.balance(&holder);
                if balance > 0 {
                    total_held += balance;
                }
            }
        }

        assert!(total_held > 0, "no eligible holders with positive balance");

        let mut distributed: i128 = 0;
        for i in 0..holders.len() {
            let holder = holders.get(i).unwrap();
            let frozen: bool = e
                .storage()
                .persistent()
                .get(&DataKey::Frozen(holder.clone()))
                .unwrap_or(false);
            if !frozen {
                let balance = token_client.balance(&holder);
                if balance > 0 {
                    let share = balance * total_amount / total_held;
                    if share > 0 {
                        token_client.transfer(&e.current_contract_address(), &holder, &share);
                        distributed = distributed
                            .checked_add(share)
                            .expect("airdrop distributed overflow");
                    }
                }
            }
        }

        let remainder = total_amount
            .checked_sub(distributed)
            .expect("airdrop remainder underflow");
        if remainder > 0 {
            token_client.transfer(&e.current_contract_address(), &admin, &remainder);
        }

        e.events()
            .publish((soroban_sdk::symbol_short!("airdrop"), admin), total_amount);
    }

    // ── #574: Permit batch ───────────────────────────────────────────────────

    fn permit_batch(
        e: Env,
        owner: Address,
        approvals: Vec<(Address, i128, u32)>,
        nonce: u64,
        public_key: BytesN<32>,
        signature: BytesN<64>,
    ) {
        permit::permit_batch(&e, owner, approvals, nonce, public_key, signature)
    }

    fn recurring_count_for_payee(e: Env, payee: Address) -> u32 {
        recurring::recurring_count_for_payee(e, payee)
    }

    fn recurring_ids_for_payee(e: Env, payee: Address) -> Vec<u32> {
        recurring::recurring_ids_for_payee(e, payee)
    }

    fn transfer_escrow_beneficiary(
        e: Env,
        depositor: Address,
        escrow_id: u32,
        new_beneficiary: Address,
    ) {
        depositor.require_auth();
        let mut record = escrow::load_record(&e, escrow_id);
        assert!(record.depositor == depositor, "not the depositor");
        let old_beneficiary = record.beneficiary.clone();
        record.beneficiary = new_beneficiary.clone();
        escrow::save_record(&e, &record);
        let ben_key = DataKey::BeneficiaryEscrows(old_beneficiary);
        let escrow_ids: Vec<u32> = e
            .storage()
            .persistent()
            .get(&ben_key)
            .unwrap_or_else(|| Vec::new(&e));
        let mut filtered: Vec<u32> = Vec::new(&e);
        for i in 0..escrow_ids.len() {
            if escrow_ids.get(i).unwrap() != escrow_id {
                filtered.push_back(escrow_ids.get(i).unwrap());
            }
        }
        e.storage().persistent().set(&ben_key, &filtered);
        let new_ben_key = DataKey::BeneficiaryEscrows(new_beneficiary);
        let mut new_ids: Vec<u32> = e
            .storage()
            .persistent()
            .get(&new_ben_key)
            .unwrap_or_else(|| Vec::new(&e));
        new_ids.push_back(escrow_id);
        e.storage().persistent().set(&new_ben_key, &new_ids);
    }

    fn total_holders(e: Env) -> u32 {
        e.storage()
            .persistent()
            .get(&DataKey::HolderCount)
            .unwrap_or(0)
    }

    fn get_holders(e: Env) -> Vec<Address> {
        e.storage()
            .persistent()
            .get(&DataKey::HolderSet)
            .unwrap_or_else(|| Vec::new(&e))
    }

    fn set_mediation_fee(e: Env, admin: Address, fee_bps: u32) {
        admin::check_admin(&e, &admin);
        e.storage()
            .persistent()
            .set(&DataKey::MediationFeeBps, &fee_bps);
    }

    fn set_authorized(e: Env, admin: Address, account: Address, authorized: bool) {
        balance::set_authorized(&e, &admin, &account, authorized)
    }

    fn increase_allowance(e: Env, from: Address, spender: Address, amount: i128) {
        allowance::increase_allowance(&e, &from, &spender, amount)
    }

    fn decrease_allowance(e: Env, from: Address, spender: Address, amount: i128) {
        allowance::decrease_allowance(&e, &from, &spender, amount)
    }

    fn revoke_all_allowances(e: Env, from: Address) {
        allowance::revoke_all_allowances(&e, &from)
    }

    fn enable_whitelist(e: Env, admin: Address) {
        whitelist::enable(&e, &admin)
    }

    fn disable_whitelist(e: Env, admin: Address) {
        whitelist::disable(&e, &admin)
    }

    fn add_to_whitelist(e: Env, admin: Address, account: Address) {
        whitelist::add(&e, &admin, &account)
    }

    fn remove_from_whitelist(e: Env, admin: Address, account: Address) {
        whitelist::remove(&e, &admin, &account)
    }

    fn is_whitelisted(e: Env, account: Address) -> bool {
        whitelist::is_whitelisted(&e, &account)
    }

    fn set_protocol_fee(e: Env, admin: Address, fee_bps: u32, treasury: Address) {
        admin::check_admin(&e, &admin);
        // #745: cap the protocol fee at 500 basis points (5%)
        assert!(fee_bps <= 500, "protocol fee cannot exceed 500 bps");
        e.storage().persistent().set(&DataKey::FeeBps, &fee_bps);
        e.storage()
            .persistent()
            .set(&DataKey::TreasuryAddress, &treasury);
    }

    fn trigger_auto_release(e: Env, escrow_id: u32) {
        escrow::trigger_auto_release(e, escrow_id)
    }

    fn transfer_recurring_payer(e: Env, caller: Address, recurring_id: u32, new_payer: Address) {
        recurring::transfer_recurring_payer(&e, &caller, recurring_id, new_payer)
    fn set_recurring_execution_window(e: Env, admin: Address, window_ledgers: u32) {
        recurring::set_recurring_execution_window(&e, &admin, window_ledgers)
    }

    fn freeze_until(e: Env, admin: Address, account: Address, until_ledger: u32) {
        crate::freeze::freeze_until(&e, &admin, &account, until_ledger)
    }

    fn add_to_whitelist_signed(
        e: Env,
        admin: Address,
        addresses: Vec<Address>,
        nonce: u64,
        public_key: BytesN<32>,
        signature: BytesN<64>,
    ) {
        whitelist::add_to_whitelist_signed(&e, &admin, &addresses, nonce, &public_key, &signature)
    }

    fn add_to_whitelist_batch(e: Env, admin: Address, accounts: Vec<Address>) {
        whitelist::add_to_whitelist_batch(&e, &admin, &accounts)
    }
}

// #773: Auxiliary contract exposing recurring history/payee views and split fee
// configuration. The four separate contract blocks merged in #773 are
// consolidated here so the crate compiles.
// NOTE: this extension contract (added by earlier merged PRs) is merged into a
// single definition here because duplicate `VeritixContract` struct/impl blocks
// from those merges broke compilation (E0428 duplicate definitions).
#[contract]
pub struct VeritixContract;

#[contractimpl]
impl VeritixContract {
    /// Retrieves the execution audit log for a specific recurring payment schedule.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `recurring_id` — recurring payment to query.
    ///
    /// # Returns
    /// A vector of [`RecurringExecution`] entries (empty if none recorded).
    pub fn get_recurring_history(e: Env, recurring_id: u32) -> Vec<RecurringExecution> {
        let key = DataKey::RecurringHistory(recurring_id);
        e.storage().instance().get(&key).unwrap_or_else(|| Vec::new(&e))
    }

    /// Retrieves all recurring payment IDs associated with a specific payee address.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `payee` — the payee to query.
    ///
    /// # Returns
    /// A vector of recurring payment IDs (empty if none exist).
    pub fn get_recurring_by_payee(e: Env, payee: Address) -> Vec<u32> {
        let key = DataKey::PayeeRecurrings(payee);
        e.storage().instance().get(&key).unwrap_or_else(|| Vec::new(&e))
    }

    /// Returns a boolean indicating whether a recurring payment schedule is currently active and not paused,
    /// avoiding the overhead of fetching the full payment record.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `recurring_id` — recurring payment to check.
    ///
    /// # Returns
    /// `true` when the schedule is active and not paused; `false` otherwise
    /// (including when the record does not exist).
    pub fn is_recurring_active(e: Env, recurring_id: u32) -> bool {
        let key = DataKey::Recurring(recurring_id);

        // Retrieve the recurring record from storage instance/persistent storage
        // (pause is modeled as `active = false`, so only `active` is checked)
        if let Some(recurring) = e
            .storage()
            .instance()
            .get::<DataKey, crate::recurring::RecurringRecord>(&key)
        {
            recurring.active
        } else {
            false
        }
    }

#[contract]
pub struct VeritixContract;

#[contractimpl]
impl VeritixContract {
    /// Sets the protocol fee and treasury address for split distributions
    /// (admin-only, max 2%).
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    /// - `admin` — the contract admin (authenticated).
    /// - `fee_bps` — split protocol fee in basis points (<= 200).
    /// - `treasury` — address receiving the split fee.
    ///
    /// # Panics
    /// - `Split protocol fee exceeds maximum allowed basis points (200)` if
    ///   `fee_bps > 200`.
    /// Sets the protocol fee and treasury address for split distributions (admin-only, max 2%).
    pub fn set_split_protocol_fee(e: Env, admin: Address, fee_bps: u32, treasury: Address) {
        crate::splitter::set_split_fee_config(&e, &admin, fee_bps, &treasury);
    }

    /// Retrieves the current split protocol fee basis points and treasury address.
    ///
    /// # Arguments
    /// - `e` — contract environment (auto-injected).
    ///
    /// # Returns
    /// `(fee_bps, treasury)` where `treasury` is `None` if not configured.
    pub fn get_split_protocol_fee(e: Env) -> (u32, Option<Address>) {
        let fee_bps: u32 = e.storage().instance().get(&DataKey::SplitProtocolFeeBps).unwrap_or(0);
        let treasury: Option<Address> = e.storage().instance().get(&DataKey::SplitProtocolTreasury);
        (fee_bps, treasury)
    }
}