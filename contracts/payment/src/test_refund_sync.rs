#![cfg(test)]

//! Tests for #809: the registered refund contract records processed refunds
//! on the payment record through `record_refund`.

use super::*;
use refund::{RefundContract, RefundContractClient, RefundReasonCode};
use soroban_sdk::{testutils::Address as _, token, Address, Env, String};

struct Fixture {
    env: Env,
    client: PaymentContractClient<'static>,
    contract_id: Address,
    admin: Address,
    customer: Address,
    merchant: Address,
    token: Address,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let contract_id = env.register(PaymentContract, ());
    let client = PaymentContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    token::StellarAssetClient::new(&env, &token).mint(&customer, &10_000i128);
    token::Client::new(&env, &token).approve(&customer, &contract_id, &10_000i128, &10_000);

    Fixture {
        env,
        client,
        contract_id,
        admin,
        customer,
        merchant,
        token,
    }
}

fn create_payment(f: &Fixture, amount: i128) -> u64 {
    f.client.create_payment(
        &f.customer,
        &f.merchant,
        &amount,
        &f.token,
        &Currency::USDC,
        &0u64,
        &String::from_str(&f.env, ""),
    )
}

fn completed_payment(f: &Fixture, amount: i128) -> u64 {
    let payment_id = create_payment(f, amount);
    f.client.complete_payment(&f.admin, &payment_id);
    payment_id
}

#[test]
fn set_refund_contract_is_admin_only() {
    let f = setup();
    let refund_contract = Address::generate(&f.env);
    let stranger = Address::generate(&f.env);

    assert_eq!(f.client.get_refund_contract(), None);
    assert_eq!(
        f.client
            .try_set_refund_contract(&stranger, &refund_contract),
        Err(Ok(Error::Basic(BasicError::Unauthorized)))
    );

    f.client.set_refund_contract(&f.admin, &refund_contract);
    assert_eq!(f.client.get_refund_contract(), Some(refund_contract));
}

#[test]
fn record_refund_only_accepts_registered_refund_contract() {
    let f = setup();
    let payment_id = completed_payment(&f, 1_000);
    let refund_contract = Address::generate(&f.env);
    let impostor = Address::generate(&f.env);

    assert_eq!(
        f.client
            .try_record_refund(&refund_contract, &payment_id, &1, &100),
        Err(Ok(Error::Payment(PaymentError::RefundContractNotSet)))
    );

    f.client.set_refund_contract(&f.admin, &refund_contract);
    assert_eq!(
        f.client.try_record_refund(&impostor, &payment_id, &1, &100),
        Err(Ok(Error::Basic(BasicError::Unauthorized)))
    );
    assert_eq!(f.client.get_payment(&payment_id).refunded_amount, 0);

    f.client
        .record_refund(&refund_contract, &payment_id, &1, &100);
    assert!(f
        .env
        .auths()
        .iter()
        .any(|(addr, _)| *addr == refund_contract));
}

#[test]
fn record_refund_tracks_partial_then_full_refund() {
    let f = setup();
    let payment_id = completed_payment(&f, 1_000);
    let refund_contract = Address::generate(&f.env);
    f.client.set_refund_contract(&f.admin, &refund_contract);

    f.client
        .record_refund(&refund_contract, &payment_id, &1, &400);
    let payment = f.client.get_payment(&payment_id);
    assert_eq!(payment.status, PaymentStatus::PartialRefunded);
    assert_eq!(payment.refunded_amount, 400);

    // The same refund id is applied at most once.
    assert_eq!(
        f.client
            .try_record_refund(&refund_contract, &payment_id, &1, &400),
        Err(Ok(Error::Payment(PaymentError::AlreadyProcessed)))
    );
    assert_eq!(
        f.client
            .try_record_refund(&refund_contract, &payment_id, &2, &700),
        Err(Ok(Error::Payment(PaymentError::RefundExceedsPayment)))
    );

    f.client
        .record_refund(&refund_contract, &payment_id, &2, &600);
    let payment = f.client.get_payment(&payment_id);
    assert_eq!(payment.status, PaymentStatus::Refunded);
    assert_eq!(payment.refunded_amount, 1_000);

    let history = f.client.get_payment_status_history(&payment_id);
    let last = history.get(history.len() - 1).unwrap();
    assert_eq!(last.status, PaymentStatus::Refunded);
    assert_eq!(last.actor, refund_contract);

    assert_eq!(
        f.client
            .try_record_refund(&refund_contract, &payment_id, &3, &1),
        Err(Ok(Error::Payment(PaymentError::AlreadyProcessed)))
    );
}

#[test]
fn record_refund_validates_amount_payment_and_status() {
    let f = setup();
    let refund_contract = Address::generate(&f.env);
    f.client.set_refund_contract(&f.admin, &refund_contract);
    let completed = completed_payment(&f, 1_000);
    let pending = create_payment(&f, 1_000);

    assert_eq!(
        f.client
            .try_record_refund(&refund_contract, &completed, &1, &0),
        Err(Ok(Error::Basic(BasicError::InvalidAmount)))
    );
    assert_eq!(
        f.client.try_record_refund(&refund_contract, &999, &1, &100),
        Err(Ok(Error::Payment(PaymentError::NotFound)))
    );
    assert_eq!(
        f.client
            .try_record_refund(&refund_contract, &pending, &1, &100),
        Err(Ok(Error::Payment(PaymentError::InvalidStatus)))
    );
}

#[test]
fn processing_a_refund_in_the_refund_contract_updates_the_payment() {
    let f = setup();
    let payment_id = completed_payment(&f, 1_000);

    let refund_contract = f.env.register(RefundContract, ());
    let refund = RefundContractClient::new(&f.env, &refund_contract);
    refund.initialize(&f.admin);
    refund.set_payment_contract_address(&f.admin, &f.contract_id);
    f.client.set_refund_contract(&f.admin, &refund_contract);
    token::StellarAssetClient::new(&f.env, &f.token).mint(&refund_contract, &1_000);

    let refund_id = refund.request_refund(
        &f.merchant,
        &payment_id,
        &f.customer,
        &400,
        &1_000,
        &f.token,
        &String::from_str(&f.env, "damaged"),
        &RefundReasonCode::CustomerRequest,
        &f.env.ledger().timestamp(),
    );
    refund.approve_refund(&f.admin, &refund_id);
    refund.process_refund(&f.admin, &refund_id);

    let payment = f.client.get_payment(&payment_id);
    assert_eq!(payment.status, PaymentStatus::PartialRefunded);
    assert_eq!(payment.refunded_amount, 400);
    assert!(refund.is_refund_synced(&refund_id));
}
