//! The README's "use it from another contract" example (tests/templates/shop), run against the burn
//! wallet: a shop that burns 2% of every sale by calling `deposit(bucket)`.

use tari_template_lib::prelude::{Amount, ComponentAddress, TARI_TOKEN};
use tari_template_test_tooling::transaction::{args, Transaction};
use tari_template_test_tooling::TemplateTest;

mod common;

const TARI: u64 = 1_000_000; // µTARI

fn setup(price: u64) -> (TemplateTest, ComponentAddress, ComponentAddress) {
    let mut test = common::template_test(&["tests/templates/shop"]);
    let proof = test.owner_proof();
    let burn_wallet: ComponentAddress = test.call_function("CaravelBurnWallet", "new", args![], vec![proof.clone()]);
    let shop: ComponentAddress =
        test.call_function("Shop", "new", args![burn_wallet, Amount::from_u64(price)], vec![proof]);
    (test, burn_wallet, shop)
}

fn buy(test: &mut TemplateTest, shop: ComponentAddress, price: u64) -> Vec<(Option<String>, String, String)> {
    let (account, proof, secret, _) = test.create_funded_account_with_keypair();
    let t = Transaction::builder_localnet(test.current_epoch())
        .call_method(account, "withdraw", args![TARI_TOKEN, Amount::from_u64(price)])
        .put_last_instruction_output_on_workspace("payment")
        .call_method(shop, "buy", args![Workspace("payment")])
        .build_and_seal(&secret);
    let result = test.execute_expect_success(t, vec![proof]);
    result
        .finalize
        .events
        .iter()
        .map(|e| (e.substate_id().map(|s| s.to_string()), e.topic().to_string(), e.payload().get_str("amount").unwrap_or_default().to_string()))
        .collect()
}

fn amount(test: &mut TemplateTest, component: ComponentAddress, method: &str) -> Amount {
    test.call_method(component, method, args![], vec![])
}

#[test]
fn a_sale_burns_two_percent() {
    let (mut test, burn_wallet, shop) = setup(50 * TARI);
    let events = buy(&mut test, shop, 50 * TARI);

    assert_eq!(amount(&mut test, burn_wallet, "balance"), Amount::from_u64(TARI)); // 2% of 50
    assert_eq!(amount(&mut test, burn_wallet, "total_deposited"), Amount::from_u64(TARI));
    assert_eq!(amount(&mut test, shop, "till"), Amount::from_u64(49 * TARI));

    // One Deposit event, emitted by the burn wallet's component (not the shop's).
    let deposits: Vec<_> = events.iter().filter(|(_, topic, _)| topic == "CaravelBurnWallet.Deposit").collect();
    assert_eq!(deposits.len(), 1);
    assert_eq!(deposits[0].0.as_deref(), Some(burn_wallet.to_string().as_str()));
    assert_eq!(deposits[0].2, TARI.to_string());
}

#[test]
fn a_sale_too_small_to_burn_still_succeeds() {
    // 2% of 49 µTARI rounds down to 0: the shop skips the burn instead of failing the sale.
    let (mut test, burn_wallet, shop) = setup(49);
    buy(&mut test, shop, 49);

    assert_eq!(amount(&mut test, burn_wallet, "balance"), Amount::zero());
    assert_eq!(amount(&mut test, shop, "till"), Amount::from_u64(49));
}
