//! Red team round 2 — ENGINE AUTHORISATION AND ACCESS CONTROL.
//!
//! Round 1 (`red_team.rs`) already showed the no-migrate `UpdateComponentTemplate` swap is blocked
//! by `OwnerRule::None`, that unknown methods are denied, and that a vault is writable only from its
//! own component's scope. These tests go further into the engine's authorisation model:
//!
//!   * `UpdateComponentTemplate` WITH a `#[migration]` function (round 1 only did the no-migrate
//!     form), and a swap onto the SAME template.
//!   * The engine component ops that take an explicit target address — `set_access_rules`,
//!     `set_owner_rule`, `set_state` — invoked from an attacker component against the burn wallet.
//!   * Method access-rule tricks: name case/unicode/whitespace variants, `deny_all` default
//!     methods, and `deposit` with wrong arguments.
//!   * The authorisation context of a nested cross-template call.
//!
//! Every attack must FAIL to rug, with a control (the same attack working on an owned/normal
//! component) wherever one is meaningful so the test cannot pass vacuously. The deployer is the
//! tooling's default key — the party a default owner rule would have empowered — and attacks run as
//! the deployer unless they say otherwise.
//!
//! Test-only templates under tests/templates (added for round 2):
//!   auth_rug_migrate  the migrate-swap payload: burn-wallet layout + #[migration] + withdraw
//!   auth_hijack       seizes another component via set_access_rules/set_owner_rule/set_state
//!   auth_caller       makes nested cross-template calls into the burn wallet

use tari_template_lib::prelude::{Amount, ComponentAddress, NonFungibleAddress, TARI_TOKEN};
use tari_template_test_tooling::crypto::RistrettoSecretKey;
use tari_template_test_tooling::transaction::builder::MainIntent;
use tari_template_test_tooling::transaction::{args, Transaction, TransactionBuilder};
use tari_template_test_tooling::TemplateTest;

mod common;

const TARI: u64 = 1_000_000; // µTARI

/// The engine's refusal for an owner-only action (RuntimeError::AccessDeniedOwnerRequired renders
/// the action as "component.update_template").
const OWNER_REQUIRED: &str = "component.update_template";

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
        "tests/templates/auth_rug_migrate",
        "tests/templates/auth_hijack",
        "tests/templates/auth_caller",
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

// ── A. UpdateComponentTemplate WITH a migration function ─────────────────────

/// CONTROL: an owned vault (default owner = deployer) can be rugged by a migrate-swap. The migration
/// keeps the vault; the new template's withdraw then drains it. Proves the migrate attack is real.
#[test]
fn migrate_swap_control_owned_vault_is_rugged() {
    let mut test = new_test();
    let owned = deploy(&mut test, "OwnedVault");
    let alice = actor(&mut test);
    deposit(&mut test, owned, &alice, 500 * TARI);
    let migrate = test.get_template_address("RugMigrate");

    let t = tx(&test)
        .update_component_template_address_with_migrate(owned, migrate, "steal", args![])
        .call_method(owned, "withdraw", args![Amount::from_u64(500 * TARI)])
        .put_last_instruction_output_on_workspace("loot")
        .call_method(alice.account, "deposit", args![Workspace("loot")]);
    let reason = as_deployer(&mut test, t);
    assert_eq!(reason, "", "the control should be ruggable by a migrate-swap");
    assert_eq!(balance(&mut test, owned), Amount::zero());
}

/// The burn wallet cannot be migrate-swapped — as the deployer, then as a stranger. The ownership
/// check runs after the migration frame is pushed but before the migration body executes, so
/// `OwnerRule::None` blocks it and the migration never runs.
#[test]
fn migrate_swap_burn_wallet_denied() {
    let mut test = new_test();
    let (wallet, alice) = funded_burn_wallet(&mut test);
    let migrate = test.get_template_address("RugMigrate");

    // As the deployer: migrate-swap then withdraw in one transaction.
    let t = tx(&test)
        .update_component_template_address_with_migrate(wallet, migrate, "steal", args![])
        .call_method(wallet, "withdraw", args![Amount::from_u64(500 * TARI)])
        .put_last_instruction_output_on_workspace("loot")
        .call_method(alice.account, "deposit", args![Workspace("loot")]);
    let reason = as_deployer(&mut test, t);
    assert!(reason.contains(OWNER_REQUIRED), "deployer migrate-swap unexpectedly: {reason}");

    // Migrate-swap alone, as the deployer.
    let swap_only = tx(&test).update_component_template_address_with_migrate(wallet, migrate, "steal", args![]);
    let reason = as_deployer(&mut test, swap_only);
    assert!(reason.contains(OWNER_REQUIRED), "deployer migrate-swap-only unexpectedly: {reason}");

    // As a stranger.
    let mallory = actor(&mut test);
    let t = tx(&test)
        .update_component_template_address_with_migrate(wallet, migrate, "steal", args![])
        .build_and_seal(&mallory.secret);
    let reason = test.execute_expect_failure(t, vec![mallory.proof.clone()]).to_string();
    assert!(reason.contains(OWNER_REQUIRED), "stranger migrate-swap unexpectedly: {reason}");

    // Still the burn wallet, still holding everything.
    let component = test.read_only_state_store().get_component(wallet).unwrap();
    assert_eq!(component.template_address(), &test.get_template_address("CaravelBurnWallet"));
    assert_eq!(balance(&mut test, wallet), Amount::from_u64(500 * TARI));
}

/// Swapping the burn wallet onto its OWN template (a no-op in effect) is still an owner-gated
/// `UpdateComponentTemplate` and is refused. There is no "same template" fast path that skips auth.
#[test]
fn swap_to_same_template_denied() {
    let mut test = new_test();
    let (wallet, _alice) = funded_burn_wallet(&mut test);
    let same = test.get_template_address("CaravelBurnWallet");

    let t = tx(&test).update_component_template(wallet, same);
    let reason = as_deployer(&mut test, t);
    assert!(reason.contains(OWNER_REQUIRED), "same-template swap unexpectedly: {reason}");
    assert_eq!(balance(&mut test, wallet), Amount::from_u64(500 * TARI));
}

/// A migrate-swap that names a function the target template does not have fails too — and the burn
/// wallet is untouched. (This fails at migration-function resolution, before execution; it is here
/// to confirm no variant of the instruction slips a state change past the owner gate.)
#[test]
fn migrate_swap_with_unknown_function_denied() {
    let mut test = new_test();
    let (wallet, _alice) = funded_burn_wallet(&mut test);
    let migrate = test.get_template_address("RugMigrate");

    let t = tx(&test).update_component_template_address_with_migrate(wallet, migrate, "balance", args![]);
    let reason = as_deployer(&mut test, t);
    // "balance" exists but is not a #[migration] function → rejected (NotAMigrationFunction), or the
    // owner gate; either way the swap does not happen.
    assert!(!reason.is_empty(), "unknown-function migrate-swap unexpectedly accepted");
    let component = test.read_only_state_store().get_component(wallet).unwrap();
    assert_eq!(component.template_address(), &test.get_template_address("CaravelBurnWallet"));
    assert_eq!(balance(&mut test, wallet), Amount::from_u64(500 * TARI));
}

// ── B. Cross-component engine ops against the burn wallet ────────────────────

/// An attacker component tries to open the burn wallet's method rules / seize its owner rule /
/// overwrite its state, naming the burn wallet as the target. The engine operates each op on the
/// attacker's own write lock and refuses because the target is not that locked component. The
/// controls show the same ops succeed on the attacker's OWN component, so the refusals are real.
#[test]
fn attacker_cannot_hijack_the_burn_wallet_via_component_ops() {
    let mut test = new_test();
    let (wallet, _alice) = funded_burn_wallet(&mut test);
    let hijack: ComponentAddress = test.call_function("Hijack", "new", args![], vec![]);

    // set_access_rules / set_owner_rule on another component → SubstateNotLocked(target).
    let t = tx(&test).call_method(hijack, "open_access_rules_on", args![wallet]);
    let reason = as_deployer(&mut test, t);
    assert!(reason.contains("not locked"), "set_access_rules on burn wallet unexpectedly: {reason}");

    let t = tx(&test).call_method(hijack, "seize_owner_rule_on", args![wallet]);
    let reason = as_deployer(&mut test, t);
    assert!(reason.contains("not locked"), "set_owner_rule on burn wallet unexpectedly: {reason}");

    // set_state on another component → AccessDeniedSetComponentState.
    let t = tx(&test).call_method(hijack, "overwrite_state_on", args![wallet, Amount::from_u64(999_999 * TARI)]);
    let reason = as_deployer(&mut test, t);
    assert!(
        reason.contains("set state on component"),
        "set_state on burn wallet unexpectedly: {reason}"
    );

    // Controls: the same ops on the attacker's OWN component succeed.
    let t = tx(&test).call_method(hijack, "open_access_rules_on_self", args![]);
    assert_eq!(as_deployer(&mut test, t), "", "control: open own access rules should work");
    let t = tx(&test).call_method(hijack, "overwrite_own_state", args![Amount::from_u64(7)]);
    assert_eq!(as_deployer(&mut test, t), "", "control: overwrite own state should work");

    // The burn wallet is untouched: still its own template, still holding everything, rules intact.
    let component = test.read_only_state_store().get_component(wallet).unwrap();
    assert_eq!(component.template_address(), &test.get_template_address("CaravelBurnWallet"));
    assert_eq!(balance(&mut test, wallet), Amount::from_u64(500 * TARI));
}

// ── C. Method access-rule tricks ─────────────────────────────────────────────

/// Methods outside the three allow-listed names hit the `deny_all` default (or do not exist). Name
/// variants that differ by case, whitespace, fullwidth/zero-width unicode, or an empty string are
/// NOT the allow-listed `deposit` and are all refused; none moves funds. Control: exact `deposit`
/// and `balance` work.
#[test]
fn method_name_tricks_and_denied_methods_are_rejected() {
    let mut test = new_test();
    let (wallet, alice) = funded_burn_wallet(&mut test);

    let names = [
        // deny_all default / non-existent methods an attacker might hope for:
        "withdraw", "withdraw_all", "take", "recall", "set_access_rules", "set_owner_rule", "new", "",
        // look-alikes for the allow-listed "deposit":
        "Deposit", "DEPOSIT", "deposit ", " deposit", "deposit\u{200b}", "ＤＥＰＯＳＩＴ", "ⅾeposit",
    ];
    for name in names {
        let t = tx(&test).call_method(wallet, name, args![Amount::from_u64(TARI)]);
        let reason = as_deployer(&mut test, t);
        assert!(!reason.is_empty(), "method {name:?} was unexpectedly accepted");
    }

    // Controls: the real method names work (balance is callable; deposit path moves funds).
    assert_eq!(balance(&mut test, wallet), Amount::from_u64(500 * TARI));
    deposit(&mut test, wallet, &alice, TARI);
    assert_eq!(balance(&mut test, wallet), Amount::from_u64(501 * TARI));
}

/// `deposit` called with the wrong arguments (an Amount instead of a Bucket, or no arguments) is
/// rejected and moves nothing. `deposit` is `allow_all`, so this exercises argument handling past
/// the access-rule check rather than the access rule itself.
#[test]
fn deposit_with_wrong_arguments_is_rejected() {
    let mut test = new_test();
    let (wallet, _alice) = funded_burn_wallet(&mut test);

    let t = tx(&test).call_method(wallet, "deposit", args![Amount::from_u64(TARI)]);
    let reason = as_deployer(&mut test, t);
    assert!(!reason.is_empty(), "deposit(Amount) unexpectedly accepted");

    let t = tx(&test).call_method(wallet, "deposit", args![]);
    let reason = as_deployer(&mut test, t);
    assert!(!reason.is_empty(), "deposit() with no args unexpectedly accepted");

    assert_eq!(balance(&mut test, wallet), Amount::from_u64(500 * TARI));
}

// ── D. Nested cross-template call authorisation ──────────────────────────────

/// A method's access rule is checked against the callee's own frame, and the burn wallet has no
/// owner — so a caller's identity never escalates into a `deny_all` method. Depositing THROUGH an
/// intermediary component works and is still counted (control); reaching `withdraw` through the same
/// nested path is denied.
#[test]
fn nested_cross_template_call_cannot_escalate() {
    let mut test = new_test();
    let (wallet, alice) = funded_burn_wallet(&mut test);
    let caller: ComponentAddress = test.call_function("Caller", "new", args![], vec![]);

    // Control: deposit forwarded through the caller component is accepted and counted.
    let t = tx(&test)
        .call_method(alice.account, "withdraw", args![TARI_TOKEN, Amount::from_u64(3 * TARI)])
        .put_last_instruction_output_on_workspace("funds")
        .call_method(caller, "deposit_into", args![wallet, Workspace("funds")])
        .build_and_seal(&alice.secret);
    test.execute_expect_success(t, vec![alice.proof.clone()]);
    assert_eq!(balance(&mut test, wallet), Amount::from_u64(503 * TARI));

    // Attack: reach `withdraw` on the burn wallet through the nested call → denied by its method rules.
    let t = tx(&test).call_method(caller, "call_method_on", args![wallet, "withdraw".to_string(), Amount::from_u64(TARI)]);
    let reason = as_deployer(&mut test, t);
    assert!(!reason.is_empty(), "nested withdraw unexpectedly accepted");

    assert_eq!(balance(&mut test, wallet), Amount::from_u64(503 * TARI));
}
