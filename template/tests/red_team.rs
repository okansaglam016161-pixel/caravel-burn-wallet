//! Red team: attempts to rug the burn wallet that go beyond calling its methods.
//!
//! THE DEPLOYER is the tooling's default key — the key that signs `new()` — and every attack runs as
//! the deployer unless it says otherwise, because the deployer is the party a default owner rule
//! would have empowered. Test-only templates under tests/templates:
//!   helpers      outside attacks on the vault, look-alike tokens, a fake Deposit event
//!   owned_vault  the CONTROL: the burn wallet's layout with the DEFAULT owner rule
//!   rug          the code a template swap would install: the same layout plus a withdraw
//!   squatter     tries to adopt the burn wallet's vault by writing its id into its own state

use tari_template_lib::prelude::{Amount, ComponentAddress, NonFungibleAddress, TARI_TOKEN, VaultId};
use tari_template_test_tooling::crypto::RistrettoSecretKey;
use tari_template_test_tooling::transaction::builder::{named_args::NamedArg, MainIntent};
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
        "tests/templates/helpers", "tests/templates/owned_vault", "tests/templates/rug", "tests/templates/squatter",
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

// ── 1. Swapping the component's code ─────────────────────────────────────────

/// `UpdateComponentTemplate` is a transaction instruction that makes an existing component run a
/// different template. It acts on the component directly, not through its methods — and it is
/// gated on component OWNERSHIP. The control proves the attack is real; the burn wallet has no owner.
#[test]
fn template_swap_control_an_owned_vault_is_rugged_by_its_deployer() {
    let mut test = new_test();
    let vault = deploy(&mut test, "OwnedVault");
    let alice = actor(&mut test);
    deposit(&mut test, vault, &alice, 500 * TARI);
    let rug = test.get_template_address("Rug");

    let t = tx(&test)
        .update_component_template(vault, rug)
        .call_method(vault, "withdraw", args![Amount::from_u64(500 * TARI)])
        .put_last_instruction_output_on_workspace("loot")
        .call_method(alice.account, "deposit", args![Workspace("loot")]);
    let reason = as_deployer(&mut test, t);
    assert_eq!(reason, "", "the control should be ruggable");
    assert_eq!(balance(&mut test, vault), Amount::zero());
}

/// The engine's refusal for an owner-only action (RuntimeError::AccessDeniedOwnerRequired).
const OWNER_REQUIRED: &str = "component.update_template";

#[test]
fn template_swap_the_burn_wallet_cannot_be_swapped_onto_other_code() {
    let mut test = new_test();
    let (wallet, alice) = funded_burn_wallet(&mut test);
    let rug = test.get_template_address("Rug");

    // As the deployer, then as a stranger: swap and withdraw in one transaction, and swap alone.
    let t = tx(&test)
        .update_component_template(wallet, rug)
        .call_method(wallet, "withdraw", args![Amount::from_u64(500 * TARI)])
        .put_last_instruction_output_on_workspace("loot")
        .call_method(alice.account, "deposit", args![Workspace("loot")]);
    let reason = as_deployer(&mut test, t);
    assert!(reason.contains(OWNER_REQUIRED), "unexpected: {reason}");

    let swap_only = tx(&test).update_component_template(wallet, rug);
    let reason = as_deployer(&mut test, swap_only);
    assert!(reason.contains(OWNER_REQUIRED), "unexpected: {reason}");

    let mallory = actor(&mut test);
    let t = tx(&test).update_component_template(wallet, rug).build_and_seal(&mallory.secret);
    let reason = test.execute_expect_failure(t, vec![mallory.proof.clone()]).to_string();
    assert!(reason.contains(OWNER_REQUIRED), "unexpected: {reason}");

    // Still the burn wallet, still holding everything.
    let component = test.read_only_state_store().get_component(wallet).unwrap();
    assert_eq!(component.template_address(), &test.get_template_address("CaravelBurnWallet"));
    assert_eq!(balance(&mut test, wallet), Amount::from_u64(500 * TARI));
}

// ── 2. The vault, from outside ───────────────────────────────────────────────

#[test]
fn outside_attacks_on_the_vault_all_fail() {
    let mut test = new_test();
    let (wallet, alice) = funded_burn_wallet(&mut test);
    let v = vault_id(&test, wallet);
    let helpers = test.get_template_address("TestHelpers");
    let thief: ComponentAddress = test.call_function("TestHelpers", "new", args![], vec![]);
    let one = Amount::from_u64(TARI);

    // The engine's refusal for each — pinned, so an attack failing for some UNRELATED reason (which
    // would hide a real path) fails the test instead of passing it.
    const NO_CONTEXT: &str = "can only be called from within a component context"; // working_state.rs check_component_scope
    let not_owned = format!("is not owned by {thief}");
    // (name, call, returns a bucket to keep, expected refusal)
    let attacks: Vec<(&str, TransactionBuilder<MainIntent>, bool, String)> = vec![
        ("withdraw by id (function)", tx(&test).call_function(helpers, "steal", args![v, one]), true, NO_CONTEXT.into()),
        ("withdraw_all by id (function)", tx(&test).call_function(helpers, "steal_all", args![v]), true, NO_CONTEXT.into()),
        ("pay fee from vault (function)", tx(&test).call_function(helpers, "pay_fee_from", args![v, one]), false, NO_CONTEXT.into()),
        ("withdraw by id (component)", tx(&test).call_method(thief, "steal_from", args![v, one]), true, not_owned.clone()),
        ("withdraw_all by id (component)", tx(&test).call_method(thief, "steal_all_from", args![v]), true, not_owned.clone()),
        ("pay fee from vault (component)", tx(&test).call_method(thief, "pay_fee_from_component", args![v, one]), false, not_owned.clone()),
        ("recall amount", tx(&test).call_function(helpers, "recall", args![v, one]), true, "Access Denied: native.resource.Recall".into()),
        ("recall all", tx(&test).call_function(helpers, "recall_all", args![v]), true, "Access Denied: native.resource.Recall".into()),
        ("freeze the vault", tx(&test).call_function(helpers, "freeze", args![v]), false, "Access Denied: native.resource.Freeze".into()),
        ("open TARI's recall rule", tx(&test).call_function(helpers, "open_tari_recall", args![]), false, "native.resource.update_access_rule.Recall".into()),
    ];
    for (name, t, keep, expected) in attacks {
        let t = if keep {
            t.put_last_instruction_output_on_workspace("loot").call_method(alice.account, "deposit", args![Workspace("loot")])
        } else {
            t
        };
        let reason = as_deployer(&mut test, t);
        assert!(!reason.is_empty(), "attack succeeded: {name}");
        assert!(reason.contains(expected.as_str()), "{name}: refused for an unexpected reason: {reason}");
    }
    assert_eq!(balance(&mut test, wallet), Amount::from_u64(500 * TARI));

    // And deposits still work afterwards: nothing was frozen.
    deposit(&mut test, wallet, &alice, TARI);
    assert_eq!(balance(&mut test, wallet), Amount::from_u64(501 * TARI));
}

/// Raised by the independent review: write the burn wallet's vault id into an ATTACKER component's
/// own state, so the attacker component "owns" it and can withdraw. The engine only lets a component
/// store substates already in its frame's scope, and a vault id passed as an argument never is.
#[test]
fn adopting_the_vault_into_another_components_state_fails() {
    let mut test = new_test();
    let (wallet, alice) = funded_burn_wallet(&mut test);
    let v = vault_id(&test, wallet);
    let squatter_template = test.get_template_address("Squatter");

    let t = tx(&test).call_function(squatter_template, "new_holding", args![v]);
    let reason = as_deployer(&mut test, t);
    assert!(reason.contains(&format!("Substate '{v}' not in scope")), "unexpected: {reason}");

    let squatter: ComponentAddress = test.call_function("Squatter", "new", args![], vec![]);
    let t = tx(&test)
        .call_method(squatter, "adopt_and_withdraw", args![v, Amount::from_u64(TARI)])
        .put_last_instruction_output_on_workspace("loot")
        .call_method(alice.account, "deposit", args![Workspace("loot")]);
    let reason = as_deployer(&mut test, t);
    assert!(reason.contains(&format!("{v} is not owned by {squatter}")), "unexpected: {reason}");

    let t = tx(&test).call_method(squatter, "adopt", args![v]);
    let reason = as_deployer(&mut test, t);
    assert!(reason.contains(&format!("Substate '{v}' not in scope")), "unexpected: {reason}");

    assert_eq!(balance(&mut test, wallet), Amount::from_u64(500 * TARI));
}

// ── 3. Deposit edge cases ────────────────────────────────────────────────────

#[test]
fn deposit_edge_one_micro_tari_and_many_in_one_transaction() {
    let mut test = new_test();
    let wallet = deploy(&mut test, "CaravelBurnWallet");
    let alice = actor(&mut test);

    deposit(&mut test, wallet, &alice, 1);
    assert_eq!(balance(&mut test, wallet), Amount::from_u64(1));

    // Three deposits in one transaction: three events, every one counted.
    let mut b = tx(&test);
    for (i, amount) in [2u64, 3, 5].into_iter().enumerate() {
        let slot = format!("f{i}");
        b = b
            .call_method(alice.account, "withdraw", args![TARI_TOKEN, Amount::from_u64(amount)])
            .put_last_instruction_output_on_workspace(slot.clone())
            .call_method(wallet, "deposit", args![Workspace(slot)]);
    }
    let result = test.execute_expect_success(b.build_and_seal(&alice.secret), vec![alice.proof.clone()]);
    let amounts: Vec<&str> = result.finalize.events.iter()
        .filter(|e| e.topic() == "CaravelBurnWallet.Deposit")
        .filter_map(|e| e.payload().get_str("amount"))
        .collect();
    assert_eq!(amounts, ["2", "3", "5"]);
    assert_eq!(balance(&mut test, wallet), Amount::from_u64(11));
    let total: Amount = test.call_method(wallet, "total_deposited", args![], vec![]);
    assert_eq!(total, Amount::from_u64(11));
}

#[test]
fn deposit_edge_look_alike_tari_and_nfts_are_refused() {
    let mut test = new_test();
    let wallet = deploy(&mut test, "CaravelBurnWallet");
    let alice = actor(&mut test);
    let helpers = test.get_template_address("TestHelpers");

    let fakes: Vec<(&str, &str, Vec<NamedArg>)> = vec![
        ("a token whose symbol is tTARI", "mint_named", args!["tTARI".to_string(), Amount::from_u64(TARI)]),
        ("an NFT", "mint_nft", args![]),
    ];
    for (name, f, a) in fakes {
        let t = tx(&test)
            .call_function(helpers, f, a)
            .put_last_instruction_output_on_workspace("fake")
            .call_method(wallet, "deposit", args![Workspace("fake")])
            .build_and_seal(&alice.secret);
        let reason = test.execute_expect_failure(t, vec![alice.proof.clone()]).to_string();
        assert!(reason.contains("Only TARI can be deposited"), "{name}: unexpected rejection: {reason}");
    }
    assert_eq!(balance(&mut test, wallet), Amount::zero());
}

// ── 4. Calling new() again ───────────────────────────────────────────────────

#[test]
fn calling_new_again_makes_a_separate_wallet_and_touches_nothing() {
    let mut test = new_test();
    let (first, alice) = funded_burn_wallet(&mut test);
    let second = deploy(&mut test, "CaravelBurnWallet");
    assert_ne!(first, second);
    assert_ne!(vault_id(&test, first), vault_id(&test, second));
    assert_eq!(balance(&mut test, second), Amount::zero());
    deposit(&mut test, second, &alice, 7);
    assert_eq!(balance(&mut test, first), Amount::from_u64(500 * TARI));
    assert_eq!(balance(&mut test, second), Amount::from_u64(7));
}

// ── 5. Spoofed events ────────────────────────────────────────────────────────

/// The topic is "<template name>.<topic>", and template names are not unique. What the engine
/// fills in and nothing can forge is the emitting component and template address — so a deposit
/// history must filter on the burn wallet's component address, never on the topic.
#[test]
fn a_deposit_event_names_the_emitting_component_and_template() {
    let mut test = new_test();
    let wallet = deploy(&mut test, "CaravelBurnWallet");
    let alice = actor(&mut test);
    let helpers = test.get_template_address("TestHelpers");
    let burn_template = test.get_template_address("CaravelBurnWallet");

    let t = tx(&test)
        .call_method(alice.account, "withdraw", args![TARI_TOKEN, Amount::from_u64(4)])
        .put_last_instruction_output_on_workspace("funds")
        .call_method(wallet, "deposit", args![Workspace("funds")])
        .call_function(helpers, "fake_deposit", args![Amount::from_u64(1_000_000 * TARI)])
        .build_and_seal(&alice.secret);
    let result = test.execute_expect_success(t, vec![alice.proof.clone()]);
    let deposits: Vec<_> = result.finalize.events.iter().filter(|e| e.topic().ends_with(".Deposit")).collect();
    assert_eq!(deposits.len(), 2, "{:?}", result.finalize.events);

    let real: Vec<_> = deposits.iter()
        .filter(|e| e.substate_id().and_then(|id| id.as_component_address()) == Some(wallet))
        .collect();
    assert_eq!(real.len(), 1);
    assert_eq!(*real[0].template_address(), burn_template);
    assert_eq!(real[0].payload().get_str("amount"), Some("4"));

    let fake = deposits.iter().find(|e| *e.template_address() == helpers).expect("the fake event");
    assert_eq!(fake.substate_id(), None, "a function call has no component to claim");
}
