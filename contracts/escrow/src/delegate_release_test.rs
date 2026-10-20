#![cfg(test)]

use crate::*;
use soroban_sdk::testutils::Ledger;
use soroban_sdk::{testutils::Address as _, token, Address, Env};

const ESCROW_AMOUNT: i128 = 1_000;

struct Setup<'a> {
    env: Env,
    client: EscrowContractClient<'a>,
    contract_id: Address,
    admin: Address,
    customer: Address,
    merchant: Address,
    delegate: Address,
    token: Address,
    token_client: token::Client<'a>,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_client = token::Client::new(&env, &token);
    let token_admin = token::StellarAssetClient::new(&env, &token);

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let delegate = Address::generate(&env);
    token_admin.mint(&customer, &ESCROW_AMOUNT);

    Setup {
        env,
        client,
        contract_id,
        admin,
        customer,
        merchant,
        delegate,
        token,
        token_client,
    }
}

fn create_escrow(s: &Setup) -> u64 {
    s.client.create_escrow(
        &s.customer,
        &s.merchant,
        &ESCROW_AMOUNT,
        &s.token,
        &0_u64,
        &0_u64,
        &0_u64,
        &false,
    )
}

#[test]
fn test_merchant_can_set_release_delegate() {
    let s = setup();
    let escrow_id = create_escrow(&s);

    s.client
        .set_release_delegate(&s.merchant, &escrow_id, &s.delegate);

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.release_delegate, Some(s.delegate.clone()));
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_customer_cannot_set_release_delegate() {
    let s = setup();
    let escrow_id = create_escrow(&s);

    s.client
        .set_release_delegate(&s.customer, &escrow_id, &s.delegate);
}

#[test]
fn test_merchant_can_remove_release_delegate() {
    let s = setup();
    let escrow_id = create_escrow(&s);

    s.client
        .set_release_delegate(&s.merchant, &escrow_id, &s.delegate);
    s.client.remove_release_delegate(&s.merchant, &escrow_id);

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.release_delegate, None);
}

#[test]
fn test_delegate_can_approve_release() {
    let s = setup();
    let escrow_id = create_escrow(&s);

    s.client
        .set_release_delegate(&s.merchant, &escrow_id, &s.delegate);
    s.client
        .approve_release_as_delegate(&s.delegate, &escrow_id);

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Released);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_non_delegate_cannot_approve_release() {
    let s = setup();
    let escrow_id = create_escrow(&s);
    let non_delegate = Address::generate(&s.env);

    s.client
        .set_release_delegate(&s.merchant, &escrow_id, &s.delegate);
    s.client
        .approve_release_as_delegate(&non_delegate, &escrow_id);
}

#[test]
#[should_panic(expected = "ReleaseDelegateNotSet")]
fn test_cannot_approve_release_without_delegate_set() {
    let s = setup();
    let escrow_id = create_escrow(&s);

    s.client
        .approve_release_as_delegate(&s.delegate, &escrow_id);
}
