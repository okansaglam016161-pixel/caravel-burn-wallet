//! Caravel Burn Wallet local tests (tari_template_test_tooling).
//!
//! THE DEPLOYER is the tooling's default key: `call_function` seals the `new()` transaction with it,
//! so it is exactly the key the default owner rule (`OwnedBySigner`) would have made the owner.
//! Every "the deployer cannot" test signs with that key and carries its proof.
//!
//! Accounts come from the tooling's own faucet, each starting with 1 000 tTARI.
//! `tests/templates/helpers` is a TEST-ONLY template: a non-TARI token, and two attacks on the
//! burn wallet's vault from outside it.

use tari_template_lib::prelude::{Amount, ComponentAddress, NonFungibleAddress, TARI_TOKEN, VaultId};
use tari_template_test_tooling::crypto::RistrettoSecretKey;
use tari_template_test_tooling::template_lib_types::SubstateOwnerRule;
use tari_template_test_tooling::transaction::builder::{named_args::NamedArg, MainIntent};
use tari_template_test_tooling::transaction::{args, Transaction, TransactionBuilder};
use tari_template_test_tooling::TemplateTest;

mod common;

const TEMPLATE: &str = "CaravelBurnWallet";
const TARI: u64 = 1_000_000; // µTARI

/// A key holder with a funded account.
struct Actor {
    account: ComponentAddress,
    proof: NonFungibleAddress,
    secret: RistrettoSecretKey,
}

fn actor(test: &mut TemplateTest) -> Actor {
    let (account, proof, secret, _public) = test.create_funded_account_with_keypair();
    Actor { account, proof, secret }
}

struct Setup {
    test: TemplateTest,
    wallet: ComponentAddress,
}

fn setup() -> Setup {
    let mut test = common::template_test(&["tests/templates/helpers"]);
    let owner_proof = test.owner_proof();
    let wallet: ComponentAddress = test.call_function(TEMPLATE, "new", args![], vec![owner_proof]);
    Setup { test, wallet }
}

fn tx(test: &TemplateTest) -> TransactionBuilder<MainIntent> {
    Transaction::builder_localnet(test.current_epoch())
}

fn deposit_tx(s: &Setup, from: &Actor, amount: u64) -> Transaction {
    tx(&s.test)
        .call_method(from.account, "withdraw", args![TARI_TOKEN, Amount::from_u64(amount)])
        .put_last_instruction_output_on_workspace("funds")
        .call_method(s.wallet, "deposit", args![Workspace("funds")])
        .build_and_seal(&from.secret)
}

fn read(s: &mut Setup, method: &str) -> Amount {
    let w = s.wallet;
    s.test.call_method(w, method, args![], vec![])
}

fn balance(s: &mut Setup) -> Amount {
    read(s, "balance")
}

fn total(s: &mut Setup) -> Amount {
    read(s, "total_deposited")
}

/// The vault the burn wallet holds — its only one.
fn vault_id(s: &Setup) -> VaultId {
    let state = s.test.read_only_state_store().inspect_component(s.wallet).unwrap();
    assert_eq!(state.vault_ids().len(), 1, "the burn wallet holds exactly one vault");
    state.vault_ids()[0]
}

/// How an attempt is made: just the call, or the call with whatever it returns deposited into
/// `loot_to` — the way a thief would actually do it, since a returned bucket left unhandled makes
/// the transaction fail on its own and would hide a working withdraw.
#[derive(Clone, Copy)]
enum Attempt {
    Plain,
    KeepOutput(ComponentAddress),
}

fn attempt_tx(s: &Setup, method: &str, a: Vec<NamedArg>, how: Attempt) -> TransactionBuilder<MainIntent> {
    let b = tx(&s.test).call_method(s.wallet, method, a);
    match how {
        Attempt::Plain => b,
        Attempt::KeepOutput(to) => b
            .put_last_instruction_output_on_workspace("loot")
            .call_method(to, "deposit", args![Workspace("loot")]),
    }
}

/// Call `method` on the burn wallet as the DEPLOYER (the key that signed `new()`), with its proof.
fn as_deployer(s: &mut Setup, method: &str, a: Vec<NamedArg>, how: Attempt) -> Result<(), String> {
    let t = attempt_tx(s, method, a, how).build_and_seal(s.test.secret_key());
    let proof = s.test.owner_proof();
    outcome(s, t, vec![proof])
}

/// Call `method` on the burn wallet as `who`.
fn as_actor(s: &mut Setup, who: &Actor, method: &str, a: Vec<NamedArg>, how: Attempt) -> Result<(), String> {
    let t = attempt_tx(s, method, a, how).build_and_seal(&who.secret);
    outcome(s, t, vec![who.proof.clone()])
}

fn outcome(s: &mut Setup, t: Transaction, proofs: Vec<NonFungibleAddress>) -> Result<(), String> {
    let result = s.test.execute_and_commit_on_success(t, proofs);
    match result.finalize.result.any_accept() {
        Some(_) if result.finalize.result.is_accept() => Ok(()),
        _ => Err(format!("{:?}", result.finalize.result)),
    }
}

// ── Deposits ─────────────────────────────────────────────────────────────────

#[test]
fn deposit_grows_balance_and_total_and_emits_the_amount() {
    let mut s = setup();
    let alice = actor(&mut s.test);
    assert_eq!(balance(&mut s), Amount::zero());
    assert_eq!(total(&mut s), Amount::zero());

    let t = deposit_tx(&s, &alice, 250 * TARI);
    let result = s.test.execute_expect_success(t, vec![alice.proof.clone()]);
    assert_eq!(balance(&mut s), Amount::from_u64(250 * TARI));
    assert_eq!(total(&mut s), Amount::from_u64(250 * TARI));

    // One Deposit event, from this component, carrying the amount.
    let deposits: Vec<_> = result.finalize.events.iter().filter(|e| e.topic().ends_with("Deposit")).collect();
    assert_eq!(deposits.len(), 1, "events: {:?}", result.finalize.events);
    assert_eq!(deposits[0].payload().get_str("amount"), Some("250000000"));
    assert_eq!(deposits[0].substate_id().and_then(|id| id.as_component_address()), Some(s.wallet));

    let t = deposit_tx(&s, &alice, 100 * TARI);
    s.test.execute_expect_success(t, vec![alice.proof.clone()]);
    assert_eq!(balance(&mut s), Amount::from_u64(350 * TARI));
    assert_eq!(total(&mut s), Amount::from_u64(350 * TARI));
}

#[test]
fn anyone_can_deposit_any_stranger() {
    let mut s = setup();
    let (stranger1, stranger2) = (actor(&mut s.test), actor(&mut s.test));
    for (who, amount) in [(&stranger1, 10 * TARI), (&stranger2, 20 * TARI)] {
        let t = deposit_tx(&s, who, amount);
        s.test.execute_expect_success(t, vec![who.proof.clone()]);
    }
    assert_eq!(total(&mut s), Amount::from_u64(30 * TARI));
    assert_eq!(balance(&mut s), Amount::from_u64(30 * TARI));
}

#[test]
fn a_non_tari_deposit_is_refused() {
    let mut s = setup();
    let alice = actor(&mut s.test);
    let helpers = s.test.get_template_address("TestHelpers");

    let t = tx(&s.test)
        .call_function(helpers, "mint", args![Amount::from_u64(1_000)])
        .put_last_instruction_output_on_workspace("other")
        .call_method(s.wallet, "deposit", args![Workspace("other")])
        .build_and_seal(&alice.secret);
    let reason = s.test.execute_expect_failure(t, vec![alice.proof.clone()]).to_string();
    assert!(reason.contains("Only TARI can be deposited"), "unexpected rejection: {reason}");
    assert_eq!(balance(&mut s), Amount::zero());
    assert_eq!(total(&mut s), Amount::zero());
}

#[test]
fn an_empty_deposit_is_refused() {
    let mut s = setup();
    let alice = actor(&mut s.test);
    let t = deposit_tx(&s, &alice, 0);
    let reason = s.test.execute_expect_failure(t, vec![alice.proof.clone()]).to_string();
    assert!(reason.contains("Deposit must be greater than zero"), "unexpected rejection: {reason}");
}

// ── Nothing can leave ────────────────────────────────────────────────────────

/// The template's whole surface. Anything added here — a withdraw, a setter, an admin method —
/// fails this test, whatever access rule it would have been given.
#[test]
fn nothing_can_leave_the_template_exposes_only_deposit_and_reads() {
    let s = setup();
    let module = s.test.get_module(TEMPLATE);
    let mut functions: Vec<&str> = module.template_def().functions().iter().map(|f| f.name.as_str()).collect();
    functions.sort_unstable();
    assert_eq!(functions, ["balance", "deposit", "new", "total_deposited"]);
}

/// The component as created: no owner, and the method rules are the three public methods plus
/// deny-by-default.
#[test]
fn nothing_can_leave_no_owner_and_deny_by_default() {
    let s = setup();
    let component = s.test.read_only_state_store().get_component(s.wallet).unwrap();
    assert_eq!(*component.owner_rule(), SubstateOwnerRule::None);

    let rules = component.access_rules();
    let mut methods: Vec<&str> = rules.method_access_rules_iter().map(|(m, _)| m.as_str()).collect();
    methods.sort_unstable();
    assert_eq!(methods, ["balance", "deposit", "total_deposited"]);
    assert_eq!(format!("{:?}", rules.get_method_access_rule("withdraw")), "DenyAll");
}

const ATTEMPTS: [&str; 14] = [
    "withdraw", "withdraw_all", "take", "take_all", "recall", "burn", "transfer", "send", "pay_fee",
    "set_access_rules", "set_owner_rule", "set_owner", "add_owner", "upgrade",
];

fn amount_arg() -> Vec<NamedArg> {
    args![Amount::from_u64(TARI)]
}

#[test]
fn nothing_can_leave_the_deployer_cannot_take_funds_or_change_the_rules() {
    let mut s = setup();
    let alice = actor(&mut s.test);
    let t = deposit_tx(&s, &alice, 500 * TARI);
    s.test.execute_expect_success(t, vec![alice.proof.clone()]);

    for method in ATTEMPTS {
        for how in [Attempt::Plain, Attempt::KeepOutput(alice.account)] {
            for a in [args![], amount_arg()] {
                let r = as_deployer(&mut s, method, a, how);
                assert!(r.is_err(), "deployer `{method}` succeeded");
            }
        }
    }
    assert_eq!(balance(&mut s), Amount::from_u64(500 * TARI));
    assert_eq!(total(&mut s), Amount::from_u64(500 * TARI));

    // The deployer keeps exactly a stranger's powers: the public reads still work for it.
    let ok = as_deployer(&mut s, "balance", args![], Attempt::Plain);
    assert!(ok.is_ok(), "{ok:?}");
}

#[test]
fn nothing_can_leave_a_stranger_cannot_take_funds_or_change_the_rules() {
    let mut s = setup();
    let alice = actor(&mut s.test);
    let mallory = actor(&mut s.test);
    let t = deposit_tx(&s, &alice, 500 * TARI);
    s.test.execute_expect_success(t, vec![alice.proof.clone()]);

    for method in ATTEMPTS {
        for how in [Attempt::Plain, Attempt::KeepOutput(mallory.account)] {
            for a in [args![], amount_arg()] {
                let r = as_actor(&mut s, &mallory, method, a, how);
                assert!(r.is_err(), "stranger `{method}` succeeded");
            }
        }
    }
    assert_eq!(balance(&mut s), Amount::from_u64(500 * TARI));
}

#[test]
fn nothing_can_leave_not_by_withdrawing_from_the_vault_directly() {
    let mut s = setup();
    let alice = actor(&mut s.test);
    let t = deposit_tx(&s, &alice, 500 * TARI);
    s.test.execute_expect_success(t, vec![alice.proof.clone()]);
    let vault = vault_id(&s);
    let helpers = s.test.get_template_address("TestHelpers");

    // Another template holding the vault's id and calling withdraw on it, as the deployer.
    let t = tx(&s.test)
        .call_function(helpers, "steal", args![vault, Amount::from_u64(TARI)])
        .put_last_instruction_output_on_workspace("loot")
        .call_method(alice.account, "deposit", args![Workspace("loot")])
        .build_and_seal(s.test.secret_key());
    let proof = s.test.owner_proof();
    let reason = s.test.execute_expect_failure(t, vec![proof]).to_string();
    assert!(reason.contains("can only be called from within a component context"), "unexpected rejection: {reason}");
    assert_eq!(balance(&mut s), Amount::from_u64(500 * TARI));

    // The same withdraw from INSIDE another component's method: past the context check, stopped
    // by ownership — the vault belongs to the burn wallet, not to the calling component.
    let thief: ComponentAddress = s.test.call_function("TestHelpers", "new", args![], vec![]);
    let t = tx(&s.test)
        .call_method(thief, "steal_from", args![vault, Amount::from_u64(TARI)])
        .put_last_instruction_output_on_workspace("loot")
        .call_method(alice.account, "deposit", args![Workspace("loot")])
        .build_and_seal(s.test.secret_key());
    let proof = s.test.owner_proof();
    let reason = s.test.execute_expect_failure(t, vec![proof]).to_string();
    assert!(reason.contains("SubstateNotOwned") || reason.contains("does not own") || reason.contains("not owned"),
        "unexpected rejection: {reason}");
    assert_eq!(balance(&mut s), Amount::from_u64(500 * TARI));
}

#[test]
fn nothing_can_leave_not_by_recalling_tari() {
    let mut s = setup();
    let alice = actor(&mut s.test);
    let t = deposit_tx(&s, &alice, 500 * TARI);
    s.test.execute_expect_success(t, vec![alice.proof.clone()]);
    let vault = vault_id(&s);
    let helpers = s.test.get_template_address("TestHelpers");

    // TARI's recall rule is DenyAll, and the resource owner cannot bypass a recall rule.
    let t = tx(&s.test)
        .call_function(helpers, "recall", args![vault, Amount::from_u64(TARI)])
        .put_last_instruction_output_on_workspace("loot")
        .call_method(alice.account, "deposit", args![Workspace("loot")])
        .build_and_seal(s.test.secret_key());
    let proof = s.test.owner_proof();
    let reason = s.test.execute_expect_failure(t, vec![proof]).to_string();
    assert!(reason.to_lowercase().contains("access denied") || reason.contains("AccessDenied"), "unexpected rejection: {reason}");
    assert_eq!(balance(&mut s), Amount::from_u64(500 * TARI));
}
