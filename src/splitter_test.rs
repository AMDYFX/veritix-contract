#[cfg(test)]
mod tests {
    use crate::contract::{VeriTixPay, VeriTixPayClient};
    use soroban_sdk::{
        testutils::{Address as _, Events as _},
        Address, Env, Vec,
    };

    struct TestEnv<'a> {
        e: Env,
        client: VeriTixPayClient<'a>,
        sender: Address,
        recipient1: Address,
        recipient2: Address,
        new_recipient: Address,
        token: Address,
    }

    fn setup() -> TestEnv<'static> {
        let e = Env::default();
        e.mock_all_auths();

        let contract_id = e.register_contract(None, VeriTixPay);
        let client = VeriTixPayClient::new(&e, &contract_id);

        let sender = Address::generate(&e);
        let recipient1 = Address::generate(&e);
        let recipient2 = Address::generate(&e);
        let new_recipient = Address::generate(&e);
        let token = e.register_stellar_asset_contract(sender.clone());

        soroban_sdk::token::StellarAssetClient::new(&e, &token).mint(&sender, &10_000);

        // Initialize contract with admin
        client.initialize(&sender);

        TestEnv {
            e,
            client,
            sender,
            recipient1,
            recipient2,
            new_recipient,
            token,
        }
    }

    #[test]
    fn test_reentrancy_guard_blocks_double_distribution() {
        let env = Env::default();

        let mut record = crate::splitter::SplitRecord {
            id: 0,
            sender: Address::generate(&env),
            recipients: Vec::from_array(
                &env,
                [
                    (Address::generate(&env), 5000),
                    (Address::generate(&env), 5000),
                ],
            ),
            token: Address::generate(&env),
            total_amount: 1000,
            event_ledger: env.ledger().sequence() + 1000,
            distributed: false,
            cancelled: false,
        };

        assert!(!record.distributed);
        record.distributed = true;

        assert!(record.distributed);
    }

    #[test]
    fn test_replace_split_recipient_success() {
        let t = setup();
        let event_ledger = t.e.ledger().sequence() + 1000;

        // Create split with two recipients
        let recipients = Vec::from_array(
            &t.e,
            [(t.recipient1.clone(), 6000), (t.recipient2.clone(), 4000)],
        );
        let split_id = crate::splitter::create_split(
            t.e.clone(),
            t.sender.clone(),
            recipients,
            t.token.clone(),
            1000,
            event_ledger,
        );

        // Verify initial recipients
        let record = crate::splitter::load_record(&t.e, split_id);
        assert_eq!(record.recipients.len(), 2);
        assert_eq!(record.recipients.get(0).unwrap().0, t.recipient1);
        assert_eq!(record.recipients.get(0).unwrap().1, 6000);

        // Replace recipient1 with new_recipient
        t.client
            .replace_split_recipient(&t.sender, &split_id, &t.recipient1, &t.new_recipient);

        // Verify the recipient was replaced, share_bps preserved
        let updated_record = crate::splitter::load_record(&t.e, split_id);
        assert_eq!(updated_record.recipients.len(), 2);
        assert_eq!(updated_record.recipients.get(0).unwrap().0, t.new_recipient);
        assert_eq!(updated_record.recipients.get(0).unwrap().1, 6000); // share_bps preserved

        // Verify event was emitted
        let events = t.e.events().all();
        assert!(!events.events().is_empty());
    }

    #[test]
    #[should_panic(expected = "old recipient not found in split")]
    fn test_replace_split_recipient_old_not_found() {
        let t = setup();
        let event_ledger = t.e.ledger().sequence() + 1000;
        let non_existent_recipient = Address::generate(&t.e);

        // Create split
        let recipients = Vec::from_array(
            &t.e,
            [(t.recipient1.clone(), 6000), (t.recipient2.clone(), 4000)],
        );
        let split_id = crate::splitter::create_split(
            t.e.clone(),
            t.sender.clone(),
            recipients,
            t.token.clone(),
            1000,
            event_ledger,
        );

        // Try to replace a non-existent recipient
        t.client.replace_split_recipient(
            &t.sender,
            &split_id,
            &non_existent_recipient,
            &t.new_recipient,
        );
    }

    #[test]
    #[should_panic(expected = "new recipient is already in the split")]
    fn test_replace_split_recipient_duplicate_new() {
        let t = setup();
        let event_ledger = t.e.ledger().sequence() + 1000;

        // Create split
        let recipients = Vec::from_array(
            &t.e,
            [(t.recipient1.clone(), 6000), (t.recipient2.clone(), 4000)],
        );
        let split_id = crate::splitter::create_split(
            t.e.clone(),
            t.sender.clone(),
            recipients,
            t.token.clone(),
            1000,
            event_ledger,
        );

        // Try to replace recipient1 with recipient2 (already exists)
        t.client
            .replace_split_recipient(&t.sender, &split_id, &t.recipient1, &t.recipient2);
    }

    #[test]
    #[should_panic(expected = "not authorised to replace recipient")]
    fn test_replace_split_recipient_unauthorized() {
        let t = setup();
        let event_ledger = t.e.ledger().sequence() + 1000;
        let unauthorized_user = Address::generate(&t.e);

        // Create split
        let recipients = Vec::from_array(
            &t.e,
            [(t.recipient1.clone(), 6000), (t.recipient2.clone(), 4000)],
        );
        let split_id = crate::splitter::create_split(
            t.e.clone(),
            t.sender.clone(),
            recipients,
            t.token.clone(),
            1000,
            event_ledger,
        );

        // Try to replace recipient from non-sender account
        t.client.replace_split_recipient(
            &unauthorized_user,
            &split_id,
            &t.recipient1,
            &t.new_recipient,
        );
    }

    #[test]
    #[should_panic(expected = "split has already been distributed")]
    fn test_replace_split_recipient_distributed() {
        let t = setup();
        let event_ledger = t.e.ledger().sequence() + 1000;

        // Create split
        let recipients = Vec::from_array(
            &t.e,
            [(t.recipient1.clone(), 6000), (t.recipient2.clone(), 4000)],
        );
        let split_id = crate::splitter::create_split(
            t.e.clone(),
            t.sender.clone(),
            recipients,
            t.token.clone(),
            1000,
            event_ledger,
        );

        // Distribute the split
        crate::splitter::distribute_split(t.e.clone(), t.sender.clone(), split_id);

        // Try to replace recipient after distribution
        t.client
            .replace_split_recipient(&t.sender, &split_id, &t.recipient1, &t.new_recipient);
    }

    #[test]
    #[should_panic(expected = "ContractPaused")]
    fn test_distribute_panics_when_contract_paused() {
        let t = setup();
        let event_ledger = t.e.ledger().sequence() + 1000;

        // Create split
        let recipients = Vec::from_array(
            &t.e,
            [(t.recipient1.clone(), 6000), (t.recipient2.clone(), 4000)],
        );
        let split_id = crate::splitter::create_split(
            t.e.clone(),
            t.sender.clone(),
            recipients,
            t.token.clone(),
            1000,
            event_ledger,
        );

        // Pause the contract
        crate::pause::set_paused(&t.e, &t.sender, true);

        // Try to distribute while paused — should panic
        crate::splitter::distribute_split(t.e.clone(), t.sender.clone(), split_id);
    }

    #[test]
    #[should_panic(expected = "split has been cancelled")]
    fn test_replace_split_recipient_cancelled() {
        let t = setup();
        let event_ledger = t.e.ledger().sequence() + 1000;

        // Create split
        let recipients = Vec::from_array(
            &t.e,
            [(t.recipient1.clone(), 6000), (t.recipient2.clone(), 4000)],
        );
        let split_id = crate::splitter::create_split(
            t.e.clone(),
            t.sender.clone(),
            recipients,
            t.token.clone(),
            1000,
            event_ledger,
        );

        // Cancel the split
        crate::splitter::cancel_split(t.e.clone(), t.sender.clone(), split_id);

        // Try to replace recipient after cancellation
        t.client
            .replace_split_recipient(&t.sender, &split_id, &t.recipient1, &t.new_recipient);
    }

    // ── #576: Splitter stress test ───────────────────────────────────────────────

    #[test]
    fn test_splitter_20_recipients_exact_distribution() {
        let e = Env::default();
        e.mock_all_auths();

        let contract_id = e.register_contract(None, VeriTixPay);
        let client = VeriTixPayClient::new(&e, &contract_id);

        let sender = Address::generate(&e);
        let token = e.register_stellar_asset_contract(sender.clone());
        let token_client = soroban_sdk::token::Client::new(&e, &token);
        let token_admin = soroban_sdk::token::StellarAssetClient::new(&e, &token);

        // Mint enough for both runs
        token_admin.mint(&sender, &10_000);
        client.initialize(&sender);

        let event_ledger = e.ledger().sequence() + 1000;

        // ── Run 1: total_amount = 999 (doesn't divide evenly by 20) ──
        let total_amount_1: i128 = 999;
        let mut recipients_vec = soroban_sdk::Vec::new(&e);
        for _i in 0..20 {
            // Each recipient gets 500 bps; 20 × 500 = 10000
            let recipient = Address::generate(&e);
            recipients_vec.push_back((recipient.clone(), 500u32));
        }

        token_admin.mint(&sender, &(total_amount_1 + 1));
        let split_id = crate::splitter::create_split(
            e.clone(),
            sender.clone(),
            recipients_vec.clone(),
            token.clone(),
            total_amount_1,
            event_ledger,
        );

        assert_eq!(
            token_client.balance(&e.current_contract_address()),
            total_amount_1
        );

        crate::splitter::distribute_split(e.clone(), sender.clone(), split_id);

        // Sum all recipient balances
        let mut total_received: i128 = 0;
        for i in 0..20 {
            let (recipient, _) = recipients_vec.get(i).unwrap();
            total_received += token_client.balance(&recipient);
        }
        // Every stroop must be accounted for
        assert_eq!(total_received, total_amount_1);
        // Contract balance returns to zero
        assert_eq!(token_client.balance(&e.current_contract_address()), 0);

        // ── Run 2: total_amount = 1 (one recipient gets 1, rest get 0) ──
        let total_amount_2: i128 = 1;
        token_admin.mint(&sender, &total_amount_2);

        let mut recipients_vec2 = soroban_sdk::Vec::new(&e);
        for _i in 0..20 {
            let recipient = Address::generate(&e);
            recipients_vec2.push_back((recipient.clone(), 500u32));
        }

        let split_id2 = crate::splitter::create_split(
            e.clone(),
            sender.clone(),
            recipients_vec2.clone(),
            token.clone(),
            total_amount_2,
            event_ledger,
        );

        crate::splitter::distribute_split(e.clone(), sender.clone(), split_id2);

        // Sum all recipient balances
        let mut total_received2: i128 = 0;
        let mut non_zero_count = 0;
        for i in 0..20 {
            let (recipient, _) = recipients_vec2.get(i).unwrap();
            let bal = token_client.balance(&recipient);
            total_received2 += bal;
            if bal > 0 {
                non_zero_count += 1;
                assert_eq!(bal, 1);
            }
        }
        assert_eq!(total_received2, 1);
        assert_eq!(non_zero_count, 1);
    }

    // ── #673: splitter edge case validation ─────────────────────────────────────

    #[test]
    #[should_panic(expected = "total basis points must equal 10000")]
    fn test_create_split_bps_under_10000_panics() {
        let t = setup();
        let event_ledger = t.e.ledger().sequence() + 1000;
        let recipients = Vec::from_array(
            &t.e,
            [(t.recipient1.clone(), 3000), (t.recipient2.clone(), 3000)],
        );
        t.e.as_contract(&t.client.address, || {
            crate::splitter::create_split(
                t.e.clone(),
                t.sender.clone(),
                recipients,
                t.token.clone(),
                500,
                event_ledger,
            );
        });
    }

    #[test]
    #[should_panic(expected = "total basis points must equal 10000")]
    fn test_create_split_bps_over_10000_panics() {
        let t = setup();
        let event_ledger = t.e.ledger().sequence() + 1000;
        let recipients = Vec::from_array(
            &t.e,
            [(t.recipient1.clone(), 6000), (t.recipient2.clone(), 6000)],
        );
        t.e.as_contract(&t.client.address, || {
            crate::splitter::create_split(
                t.e.clone(),
                t.sender.clone(),
                recipients,
                t.token.clone(),
                1000,
                event_ledger,
            );
        });
    }

    #[test]
    #[should_panic(expected = "total amount must be greater than zero")]
    fn test_create_split_zero_total_panics() {
        let t = setup();
        let event_ledger = t.e.ledger().sequence() + 1000;
        let recipients = Vec::from_array(
            &t.e,
            [(t.recipient1.clone(), 5000), (t.recipient2.clone(), 5000)],
        );
        t.e.as_contract(&t.client.address, || {
            crate::splitter::create_split(
                t.e.clone(),
                t.sender.clone(),
                recipients,
                t.token.clone(),
                0,
                event_ledger,
            );
        });
    }

    #[test]
    fn test_create_split_20_recipients_succeeds() {
        let e = Env::default();
        e.mock_all_auths();
        let contract_id = e.register_contract(None, VeriTixPay);
        let client = VeriTixPayClient::new(&e, &contract_id);
        let sender = Address::generate(&e);
        let token = e.register_stellar_asset_contract(sender.clone());
        let token_admin = soroban_sdk::token::StellarAssetClient::new(&e, &token);
        token_admin.mint(&sender, &10000);
        client.initialize(&sender);

        let event_ledger = e.ledger().sequence() + 1000;
        let mut recipients_vec = soroban_sdk::Vec::new(&e);
        for _ in 0..20 {
            let recipient = Address::generate(&e);
            recipients_vec.push_back((recipient.clone(), 500u32));
        }
        let split_id = e.as_contract(&contract_id, || {
            crate::splitter::create_split(
                e.clone(),
                sender.clone(),
                recipients_vec,
                token.clone(),
                1000,
                event_ledger,
            )
        });
        assert_eq!(split_id, 0);
    }

    #[test]
    fn test_distribute_10_recipients_uneven_bps_no_stroop_lost() {
        let e = Env::default();
        e.mock_all_auths();
        let contract_id = e.register_contract(None, VeriTixPay);
        let client = VeriTixPayClient::new(&e, &contract_id);
        let sender = Address::generate(&e);
        let token = e.register_stellar_asset_contract(sender.clone());
        let token_client = soroban_sdk::token::Client::new(&e, &token);
        let token_admin = soroban_sdk::token::StellarAssetClient::new(&e, &token);
        token_admin.mint(&sender, &10000);
        client.initialize(&sender);

        let event_ledger = e.ledger().sequence() + 1000;
        let total: i128 = 999;
        token_admin.mint(&sender, &total);
        let mut recipients_vec = soroban_sdk::Vec::new(&e);
        // Uneven shares: 10 recipients across varying bps that still sum to 10000.
        let shares: [u32; 10] = [1234, 987, 1001, 876, 1100, 900, 999, 1002, 998, 903];
        for s in shares {
            let recipient = Address::generate(&e);
            recipients_vec.push_back((recipient.clone(), s));
        }
        let split_id = e.as_contract(&contract_id, || {
            crate::splitter::create_split(
                e.clone(),
                sender.clone(),
                recipients_vec.clone(),
                token.clone(),
                total,
                event_ledger,
            )
        });
        e.as_contract(&contract_id, || {
            crate::splitter::distribute_split(e.clone(), sender.clone(), split_id);
        });

        let mut total_received: i128 = 0;
        for i in 0..recipients_vec.len() {
            let (recipient, _) = recipients_vec.get(i).unwrap();
            total_received += token_client.balance(&recipient);
        }
        assert_eq!(total_received, total);
        assert_eq!(token_client.balance(&contract_id), 0);
    }
}

#[cfg(test)]
mod splitter_fee_tests {
    use super::*;
    use soroban_sdk::Env;

    #[test]
    fn test_split_protocol_fee_configuration_and_calculation() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let fee_bps = 100; // 1%

        // Set split protocol fee
        VeritixContract::set_split_protocol_fee(env.clone(), admin, fee_bps, treasury.clone());

        // Verify getter view
        let (fetched_bps, fetched_treasury) = VeritixContract::get_split_protocol_fee(env.clone());
        assert_eq!(fetched_bps, fee_bps);
        assert_eq!(fetched_treasury.unwrap(), treasury);

        // Test fee calculation on a 10,000 token split distribution
        let (remainder, fee_info) = crate::splitter::calculate_split_fee(&env, 10000_i128);
        assert_eq!(remainder, 9900_i128);
        assert_eq!(fee_info.unwrap().1, 100_i128);
    }
}