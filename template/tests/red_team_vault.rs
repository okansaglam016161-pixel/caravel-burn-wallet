//! Red team round 2 — area: VAULTS / RESOURCES / FEES / STEALTH / CONFIDENTIAL.
//!
//! Round 1 (`red_team.rs`) already proved: direct withdraw/withdraw_all, recall amount / recall all,
//! freeze, the template swap, vault adoption (squatter), look-alike tokens and spoofed events all
//! fail. This file goes after the money along the fee, proof, confidential and resource-rule paths
//! that round 1 did not touch:
//!
//!   * can a transaction make the burn wallet PAY ITS FEE — as a fee instruction calling the burn
//!     component's (non-existent) `pay_fee`, or via the engine's native `Vault::pay_fee` against the
//!     burn vault from an attacker component?
//!   * can an attacker LOCK the burn vault's funds with a proof, or CONFIDENTIAL-withdraw from it?
//!   * can TARI be MINTED or BURNED, or its Deposit/Withdraw/Mint/Burn/Recall/Freeze rules be
//!     CHANGED (which would mint, destroy, or make the wallet un-depositable for everyone)?
//!
//! Every test asserts the attack is rejected AND the burn wallet's balance / total_deposited are
//! unchanged. Where a move is only blocked by component scope, a CONTROL runs the identical move
//! against a vault the attacker component really owns, so the refusal can't be vacuous.
//!
//! Templates (tests/templates): vault_feethief (fee/proof/confidential attacks + a self-owned vault
//! control), vault_resource (mint/burn/rule-change through the TARI ResourceManager). owned_vault is
//! reused from round 1 as the swap/ownership control.

use tari_template_lib::prelude::{
    Amount, ComponentAddress, NonFungibleAddress, ResourceAuthAction, TARI_TOKEN, VaultId,
};
use tari_template_test_tooling::crypto::RistrettoSecretKey;
use tari_template_test_tooling::support::confidential::generate_reveal_proof;
use tari_template_test_tooling::transaction::builder::MainIntent;
use tari_template_test_tooling::transaction::{args, Transaction, TransactionBuilder};
use tari_template_test_tooling::TemplateTest;

mod common;

const TARI: u64 = 1_000_000; // µTARI

struct Actor {
    account: ComponentAddress,
    proof: NonFungibleAddress,
    secret: RistrettoSecretKey,
}

fn actor(test: &mut TemplateTest) -> Actor {
    let (account, proof, secret, _public) = test.create_funded_account_with_keypair();
    Actor { account, proof, secret }
}

fn new_test() -> TemplateTest {
    common::template_test(&[
        "tests/templates/owned_vault",
        "tests/templates/vault_feethief",
        "tests/templates/vault_resource",
    ])
}

fn tx(test: &TemplateTest) -> TransactionBuilder<MainIntent> {
    Transaction::builder_localnet(test.current_epoch())
}

/// Create a component of `template` as the deployer.
fn deploy(test: &mut TemplateTest, template: &str) -> ComponentAddress {
    let proof = test.owner_proof();
    test.call_function(template, "new", args![], vec![proof])
}

fn deposit(test: &mut TemplateTest, into: ComponentAddress, from: &Actor, amount: u64) {
    let t = tx(test)
        .call_method(from.account, "withdraw", args![TARI_TOKEN, Amount::from_u64(amount)])
        .put_last_instruction_output_on_workspace("funds")
        .call_method(into, "deposit", args![Workspace("funds")])
        .build_and_seal(&from.secret);
    test.execute_expect_success(t, vec![from.proof.clone()]);
}

fn balance(test: &mut TemplateTest, of: ComponentAddress) -> Amount {
    test.call_method(of, "balance", args![], vec![])
}

fn total_deposited(test: &mut TemplateTest, of: ComponentAddress) -> Amount {
    test.call_method(of, "total_deposited", args![], vec![])
}

fn vault_id(test: &TemplateTest, of: ComponentAddress) -> VaultId {
    let state = test.read_only_state_store().inspect_component(of).unwrap();
    assert_eq!(state.vault_ids().len(), 1);
    state.vault_ids()[0]
}

/// Run `t` signed by the deployer; the rejection text, or "" if it was accepted.
fn as_deployer(test: &mut TemplateTest, t: TransactionBuilder<MainIntent>) -> String {
    let t = t.build_and_seal(test.secret_key());
    let proof = test.owner_proof();
    let result = test.execute_and_commit_on_success(t, vec![proof]);
    if result.finalize.result.is_accept() { String::new() } else { format!("{:?}", result.finalize.result) }
}

/// A funded burn wallet (500 TARI) and the actor who funded it.
fn funded_burn_wallet(test: &mut TemplateTest) -> (ComponentAddress, Actor) {
    let wallet = deploy(test, "CaravelBurnWallet");
    let alice = actor(test);
    deposit(test, wallet, &alice, 500 * TARI);
    (wallet, alice)
}

/// A VaultFeethief with 10 TARI in its OWN vault, for the control moves.
fn funded_feethief(test: &mut TemplateTest, funder: &Actor) -> ComponentAddress {
    let thief: ComponentAddress = test.call_function("VaultFeethief", "new", args![], vec![]);
    let t = tx(test)
        .call_method(funder.account, "withdraw", args![TARI_TOKEN, Amount::from_u64(10 * TARI)])
        .put_last_instruction_output_on_workspace("f")
        .call_method(thief, "fund", args![Workspace("f")])
        .build_and_seal(&funder.secret);
    test.execute_expect_success(t, vec![funder.proof.clone()]);
    thief
}

fn unchanged(test: &mut TemplateTest, wallet: ComponentAddress, expected: u64) {
    assert_eq!(balance(test, wallet), Amount::from_u64(expected), "balance moved");
    assert_eq!(total_deposited(test, wallet), Amount::from_u64(expected), "total_deposited moved");
}

// ── 1. The burn wallet as a fee payer (method path) ──────────────────────────

/// A transaction cannot name the burn component as its fee payer: `pay_fee` is not one of its three
/// methods, so the default `deny_all` rule refuses it — whether the call is placed as a fee
/// instruction (the normal "pay_fee_from_component" position) or as a main instruction.
#[test]
fn naming_the_burn_wallet_as_fee_payer_is_denied() {
    let mut test = new_test();
    let (wallet, _alice) = funded_burn_wallet(&mut test);

    // Fee-instruction position: pay_fee_from_component(wallet) builds CallMethod(wallet, "pay_fee").
    let as_fee = tx(&test).pay_fee_from_component(wallet, 1000u64);
    let reason = as_deployer(&mut test, as_fee);
    assert!(!reason.is_empty(), "the burn wallet paid a fee (fee-instruction position)");

    // Main-instruction position: an explicit CallMethod(wallet, "pay_fee").
    let as_main = tx(&test).call_method(wallet, "pay_fee", args![Amount::from_u64(1000)]);
    let reason = as_deployer(&mut test, as_main);
    assert!(!reason.is_empty(), "the burn wallet paid a fee (main-instruction position)");

    unchanged(&mut test, wallet, 500 * TARI);
}

// ── 2. The burn vault as a fee payer (native Vault::pay_fee path) ─────────────

/// The engine's native `Vault::pay_fee` withdraws straight from a vault. From inside an attacker
/// component the only gate left is `check_component_scope`: the CONTROL pays the fee out of the
/// thief's OWN vault (fee-instruction position) and is accepted, while the identical call naming the
/// burn vault is refused as "not owned" — in both the fee-instruction and the main-instruction
/// position (the scope check runs before the fee-intent gate either way).
#[test]
fn native_pay_fee_from_the_burn_vault_is_scope_checked() {
    let mut test = new_test();
    let (wallet, alice) = funded_burn_wallet(&mut test);
    let v = vault_id(&test, wallet);
    let thief = funded_feethief(&mut test, &alice);
    let not_owned = format!("{v} is not owned by {thief}");

    // CONTROL: pay the fee from the vault the thief owns, in the fee-instruction position — accepted.
    let control = tx(&test)
        .with_fee_instructions_builder(|b| b.call_method(thief, "pay_fee_own", args![Amount::from_u64(1000)]));
    assert_eq!(as_deployer(&mut test, control), "", "control: paying from our own vault should work");

    // ATTACK (fee-instruction position): pay the fee out of the burn vault.
    let attack_fee = tx(&test)
        .with_fee_instructions_builder(|b| b.call_method(thief, "pay_fee_foreign", args![v, Amount::from_u64(1000)]));
    let reason = as_deployer(&mut test, attack_fee);
    assert!(reason.contains(&not_owned), "unexpected (fee instr): {reason}");

    // ATTACK (main-instruction position): same call, placed as a main instruction.
    let attack_main = tx(&test).call_method(thief, "pay_fee_foreign", args![v, Amount::from_u64(1000)]);
    let reason = as_deployer(&mut test, attack_main);
    assert!(reason.contains(&not_owned), "unexpected (main instr): {reason}");

    unchanged(&mut test, wallet, 500 * TARI);
}

// ── 3. Locking the burn vault's funds with a proof ───────────────────────────

/// Creating a proof over a vault locks funds in it. Done to the burn vault from outside it could, in
/// principle, grief deposits or pin funds. It is scope-checked like any write: a foreign vault id is
/// refused, the thief's own vault accepts it, and deposits keep working afterwards.
#[test]
fn locking_the_burn_vault_with_a_proof_from_outside_fails() {
    let mut test = new_test();
    let (wallet, alice) = funded_burn_wallet(&mut test);
    let v = vault_id(&test, wallet);
    let thief = funded_feethief(&mut test, &alice);

    // CONTROL: lock our own vault's funds — accepted.
    let control = tx(&test).call_method(thief, "lock_own", args![Amount::from_u64(1000)]);
    assert_eq!(as_deployer(&mut test, control), "", "control: locking our own vault should work");

    // ATTACK: lock the burn vault's funds.
    let attack = tx(&test).call_method(thief, "lock_foreign", args![v, Amount::from_u64(1000)]);
    let reason = as_deployer(&mut test, attack);
    assert!(reason.contains(&format!("{v} is not owned by {thief}")), "unexpected: {reason}");

    // Nothing was locked: deposits still work and the balance is intact.
    unchanged(&mut test, wallet, 500 * TARI);
    deposit(&mut test, wallet, &alice, TARI);
    unchanged(&mut test, wallet, 501 * TARI);
}

// ── 4. Confidential withdraw from the burn vault ─────────────────────────────

/// A confidential withdraw is the one withdraw shape round 1 did not try. From an attacker component
/// it is refused by the same scope check (before the engine would even reach the "cannot withdraw
/// confidential assets from a stealth resource" guard, since TARI is a Stealth resource). The proof
/// is a well-formed reveal proof so the refusal is scope, not a malformed argument.
#[test]
fn confidential_withdraw_from_the_burn_vault_from_outside_fails() {
    let mut test = new_test();
    let (wallet, alice) = funded_burn_wallet(&mut test);
    let v = vault_id(&test, wallet);
    let thief = funded_feethief(&mut test, &alice);

    let mask = RistrettoSecretKey::from(123u64);
    let proof = generate_reveal_proof(&mask, 1_000);

    let attack = tx(&test)
        .call_method(thief, "withdraw_confidential_foreign", args![v, proof])
        .put_last_instruction_output_on_workspace("loot")
        .call_method(alice.account, "deposit", args![Workspace("loot")]);
    let reason = as_deployer(&mut test, attack);
    assert!(reason.contains(&format!("{v} is not owned by {thief}")), "unexpected: {reason}");

    unchanged(&mut test, wallet, 500 * TARI);
}

// ── 5. Minting TARI ──────────────────────────────────────────────────────────

/// TARI's Mint rule is DenyAll (and Locked). Minting TARI and depositing it into the burn wallet
/// must fail at the mint, so no TARI can be created to inflate the supply or the wallet.
#[test]
fn minting_tari_is_denied() {
    let mut test = new_test();
    let (wallet, _alice) = funded_burn_wallet(&mut test);

    let attack = tx(&test)
        .call_function(test.get_template_address("VaultResource"), "mint_tari", args![Amount::from_u64(1_000_000)])
        .put_last_instruction_output_on_workspace("minted")
        .call_method(wallet, "deposit", args![Workspace("minted")]);
    let reason = as_deployer(&mut test, attack);
    assert!(reason.contains("Mint") || reason.contains("Access Denied"), "unexpected: {reason}");

    unchanged(&mut test, wallet, 500 * TARI);
}

// ── 6. Burning TARI ──────────────────────────────────────────────────────────

/// TARI's Burn rule is DenyAll (and Locked). Even a holder burning their OWN TARI must be refused,
/// so no one can destroy TARI (the mechanism a griefer would use to shrink the supply around the
/// burn wallet, or that a bug might let reach the vault).
#[test]
fn burning_tari_is_denied() {
    let mut test = new_test();
    let (wallet, alice) = funded_burn_wallet(&mut test);

    let attack = tx(&test)
        .call_method(alice.account, "withdraw", args![TARI_TOKEN, Amount::from_u64(TARI)])
        .put_last_instruction_output_on_workspace("funds")
        .call_function(test.get_template_address("VaultResource"), "burn_tari", args![Workspace("funds")]);
    let reason = as_deployer(&mut test, attack);
    assert!(reason.contains("Burn") || reason.contains("Access Denied"), "unexpected: {reason}");

    unchanged(&mut test, wallet, 500 * TARI);
}

// ── 7. Changing TARI's access rules ──────────────────────────────────────────

/// Every one of TARI's rule updaters is Locked, so no rule can be changed by anyone (there is no
/// resource owner either). The most damaging would be flipping Deposit to deny_all — that would make
/// the burn wallet, and every account, unable to receive TARI ever again. Each attempt must be
/// refused, and a normal deposit must still work afterwards.
#[test]
fn changing_tari_access_rules_is_locked() {
    let mut test = new_test();
    let (wallet, alice) = funded_burn_wallet(&mut test);
    let res = test.get_template_address("VaultResource");

    // (action, allow?) — the deposit/withdraw flips are the DoS-shaped ones; the rest reopen a
    // locked-down rule so a later recall/mint/burn/freeze would work.
    let attempts: Vec<(ResourceAuthAction, bool)> = vec![
        (ResourceAuthAction::Deposit, false),  // make TARI un-depositable (DoS)
        (ResourceAuthAction::Withdraw, false), // freeze all spending (DoS)
        (ResourceAuthAction::Mint, true),      // open minting
        (ResourceAuthAction::Burn, true),      // open burning
        (ResourceAuthAction::Recall, true),    // open recall (would then drain the vault)
        (ResourceAuthAction::Freeze, true),    // open freeze (would then lock the vault)
    ];
    for (action, allow) in attempts {
        let attack = tx(&test).call_function(res, "set_tari_rule", args![action.clone(), allow]);
        let reason = as_deployer(&mut test, attack);
        assert!(!reason.is_empty(), "TARI rule change accepted for {action:?}");
        assert!(
            reason.contains("update_access_rule") || reason.contains("Locked") || reason.contains("Access Denied"),
            "{action:?}: refused for an unexpected reason: {reason}"
        );
    }

    // Deposit still works: the Deposit rule was not changed.
    unchanged(&mut test, wallet, 500 * TARI);
    deposit(&mut test, wallet, &alice, TARI);
    unchanged(&mut test, wallet, 501 * TARI);
}
