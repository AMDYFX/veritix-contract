#![cfg(test)]

use crate::contract::{VeriTixPay, VeriTixPayClient};
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

#[test]
fn test_distribute_dividend_proportional_to_balance() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);

    let h1 = Address::generate(&e);
    let h2 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        let mut holders: Vec<Address> = Vec::new(&e);
        holders.push_back(h1.clone());
        holders.push_back(h2.clone());
        crate::divi::distribute_dividend(&e, &admin, 99, holders);
    });

    // 99 / 2 = 49 per holder, remainder 1 goes to the first holder.
    assert_eq!(client.balance(&h1), 50);
    assert_eq!(client.balance(&h2), 49);
}

#[test]
fn test_distribute_dividend_remainder_returned_to_first_holder() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);

    let h1 = Address::generate(&e);
    let h2 = Address::generate(&e);
    let h3 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        let mut holders: Vec<Address> = Vec::new(&e);
        holders.push_back(h1.clone());
        holders.push_back(h2.clone());
        holders.push_back(h3.clone());
        // 100/3 -> base 33 each, remainder 1 to first holder
        crate::divi::distribute_dividend(&e, &admin, 100, holders);
    });

    assert_eq!(client.balance(&h1), 34);
    assert_eq!(client.balance(&h2), 33);
    assert_eq!(client.balance(&h3), 33);
}

#[test]
#[should_panic(expected = "total_dividend must be positive")]
fn test_distribute_dividend_zero_total_panics() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);

    let h1 = Address::generate(&e);
    e.as_contract(&contract_id, || {
        let mut holders: Vec<Address> = Vec::new(&e);
        holders.push_back(h1);
        crate::divi::distribute_dividend(&e, &admin, 0, holders);
    });
}

#[test]
fn test_distribute_dividend_empty_holders_is_noop() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);

    e.as_contract(&contract_id, || {
        let holders: Vec<Address> = Vec::new(&e);
        crate::divi::distribute_dividend(&e, &admin, 1000, holders);
    });

    assert_eq!(client.total_supply(), 0);
}

#[test]
fn test_distribute_dividend_single_holder_gets_all() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, VeriTixPay);
    let client = VeriTixPayClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);

    let h1 = Address::generate(&e);
    e.as_contract(&contract_id, || {
        let mut holders: Vec<Address> = Vec::new(&e);
        holders.push_back(h1.clone());
        crate::divi::distribute_dividend(&e, &admin, 777, holders);
    });
    assert_eq!(client.balance(&h1), 777);
}
