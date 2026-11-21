#![cfg(test)]

use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events},
    Address, Env, IntoVal, Symbol, TryFromVal, Val, Vec,
};

#[test]
fn test_event_topics_and_data_encoding() {
    let e = Env::default();
    let contract_id = Address::generate(&e);
    let user = Address::generate(&e);

    e.as_contract(&contract_id, || {
        let topics: Vec<Val> = (symbol_short!("transfer"), user.clone()).into_val(&e);
        let data: Val = 1_000_i128.into_val(&e);
        e.events().publish(topics, data);
    });

    let events = e.events().all();
    assert_eq!(events.len(), 1);

    let (emitted_contract, topics, data) = events.get(0).unwrap();
    assert_eq!(emitted_contract, contract_id);

    let expected_topics: Vec<Val> = (symbol_short!("transfer"), user).into_val(&e);
    assert_eq!(topics, expected_topics);

    let decoded_amount = i128::try_from_val(&e, &data).unwrap();
    assert_eq!(decoded_amount, 1_000_i128);
}

#[test]
fn test_admin_rotation_event_payload() {
    let e = Env::default();
    let contract_id = Address::generate(&e);
    let old_admin = Address::generate(&e);
    let new_admin = Address::generate(&e);

    e.as_contract(&contract_id, || {
        let topics: Vec<Val> = (
            Symbol::new(&e, "admin_transfer"),
            old_admin.clone(),
            new_admin.clone(),
        )
            .into_val(&e);
        let payload: Val = e.ledger().sequence().into_val(&e);
        e.events().publish(topics, payload);
    });

    let events = e.events().all();
    assert_eq!(events.len(), 1);

    let (_contract, topics, _data) = events.get(0).unwrap();
    let expected_topics: Vec<Val> =
        (Symbol::new(&e, "admin_transfer"), old_admin, new_admin).into_val(&e);
    assert_eq!(topics, expected_topics);
}

#[test]
fn test_empty_event_vec_type_annotations() {
    let e = Env::default();
    let empty_topics: Vec<Val> = Vec::new(&e);
    assert_eq!(empty_topics.len(), 0);
}