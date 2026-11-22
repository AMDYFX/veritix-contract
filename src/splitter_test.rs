#[test]
    fn test_replace_split_recipient_updates_address() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, VeritixContract);
        let client = VeritixContractClient::new(&env, &contract_id);

        let sender = Address::generate(&env);
        let old_r = Address::generate(&env);
        let new_r = Address::generate(&env);

        let split_id = env.as_contract(&contract_id, || {
            Splitter::create_split(
                &env,
                sender.clone(),
                vec![&env, SplitRecipient { recipient: old_r.clone(), bps: 10_000 }],
            )
            .unwrap()
        });

        client.replace_split_recipient(&split_id, &old_r, &new_r);

        env.as_contract(&contract_id, || {
            let record: SplitRecord = env
                .storage()
                .persistent()
                .get(&SplitterDataKey::Split(split_id))
                .unwrap();
            assert_eq!(record.recipients.get(0).unwrap().recipient, new_r);
        });
    }

    #[test]
    #[should_panic(expected = "duplicate recipient in split")]
    fn test_replace_split_recipient_panics_on_duplicate() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, VeritixContract);
        let client = VeritixContractClient::new(&env, &contract_id);

        let sender = Address::generate(&env);
        let r1 = Address::generate(&env);
        let r2 = Address::generate(&env);

        let split_id = env.as_contract(&contract_id, || {
            Splitter::create_split(
                &env,
                sender.clone(),
                vec![
                    &env,
                    SplitRecipient { recipient: r1.clone(), bps: 6_000 },
                    SplitRecipient { recipient: r2.clone(), bps: 4_000 },
                ],
            )
            .unwrap()
        });

        // Replacing r1 with r2 (already present) must panic
        client.replace_split_recipient(&split_id, &r1, &r2);
    }

    #[test]
    #[should_panic(expected = "9999")]
    fn test_create_split_rejects_total_below_10000_bps() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, DummySplitterContract);
        let sender = Address::generate(&env);
        let r1 = Address::generate(&env);
        let r2 = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let _ = Splitter::create_split(
                &env,
                sender,
                vec![
                    &env,
                    SplitRecipient { recipient: r1, bps: 5_000 },
                    SplitRecipient { recipient: r2, bps: 4_999 },
                ],
            );
        });
    }

    #[test]
    #[should_panic(expected = "10001")]
    fn test_create_split_rejects_total_above_10000_bps() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, DummySplitterContract);
        let sender = Address::generate(&env);
        let r1 = Address::generate(&env);
        let r2 = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let _ = Splitter::create_split(
                &env,
                sender,
                vec![
                    &env,
                    SplitRecipient { recipient: r1, bps: 5_000 },
                    SplitRecipient { recipient: r2, bps: 5_001 },
                ],
            );
        });
    }