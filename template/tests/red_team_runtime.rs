//! Red team round 2 — RUNTIME / transaction processing / substate versioning / dry-run divergence.
//!
//! Round 1 (`red_team.rs`, `burn_wallet.rs`) proved the method/owner surface. This file attacks the
//! burn wallet through the TRANSACTION PROCESSOR instead of its methods: the dry-run flag, the new
//! 0.45 migrate-on-swap path, instruction ordering and the workspace, atomic abort/rollback, and
//! value conservation across bucket splits. Every test is an attack that must FAIL to rug: the
//! transaction is rejected (or succeeds harmlessly) AND the burn wallet's balance / total_deposited
//! are unchanged. Controls (an owned/normal component the same attack DOES move) keep the tests from
//! passing vacuously.
//!
//! Substate-version races, cross-shard foreign-input mutation, transaction replay (tx-id/nonce
//! reuse) and input-lock griefing are CONSENSUS-layer properties that the single-node WASM test
//! harness does not model (it commits nothing and re-derives inputs at the current version on every
//! call). Those are covered by the code read in REPORT.md, not by a misleading local assertion.

use tari_template_lib::prelude::{Amount, ComponentAddress, NonFungibleAddress, TARI_TOKEN, VaultId};
use tari_template_test_tooling::crypto::RistrettoSecretKey;
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
        "tests/templates/helpers",
        "tests/templates/owned_vault",
        "tests/templates/rug",
        "tests/templates/runtime_migrate",
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

fn total(test: &mut TemplateTest, of: ComponentAddress) -> Amount {
    test.call_method(of, "total_deposited", args![], vec![])
}

fn vault_id(test: &TemplateTest, of: ComponentAddress) -> VaultId {
    let state = test.read_only_state_store().inspect_component(of).unwrap();
    assert_eq!(state.vault_ids().len(), 1);
    state.vault_ids()[0]
}

/// Run `t` signed by the deployer, committing the diff if it is accepted; return the rejection text,
/// or "" if it was accepted.
fn as_deployer(test: &mut TemplateTest, t: TransactionBuilder<MainIntent>) -> String {
    let t = t.build_and_seal(test.secret_key());
    let proof = test.owner_proof();
    let result = test.execute_and_commit_on_success(t, vec![proof]);
    if result.finalize.result.is_accept() { String::new() } else { format!("{:?}", result.finalize.result) }
}

/// Run `t` signed by `who`, committing the diff if it is accepted; return the rejection text, or "".
fn as_actor(test: &mut TemplateTest, who: &Actor, t: TransactionBuilder<MainIntent>) -> String {
    let t = t.build_and_seal(&who.secret);
    let result = test.execute_and_commit_on_success(t, vec![who.proof.clone()]);
    if result.finalize.result.is_accept() { String::new() } else { format!("{:?}", result.finalize.result) }
}

/// The engine's refusal for an owner-only action (RuntimeError::AccessDeniedOwnerRequired).
const OWNER_REQUIRED: &str = "component.update_template";

/// A funded burn wallet (500 TARI) and the actor who funded it.
fn funded_burn_wallet(test: &mut TemplateTest) -> (ComponentAddress, Actor) {
    let wallet = deploy(test, "CaravelBurnWallet");
    let alice = actor(test);
    deposit(test, wallet, &alice, 500 * TARI);
    (wallet, alice)
}

// ── 1. Dry-run flag does not weaken auth (the thing we rely on) ───────────────

/// We rely on dry runs for safety. A dry run that ACCEPTS a drain the real submit would reject
/// would be critical. The engine's `dry_run` flag (TemplateTest::set_dry_run -> the same flag the
/// indexer passes TransactionProcessor::new) must change NOTHING about owner/access-rule checks:
/// the ownerless template swap + withdraw must be refused identically in dry and real execution.
/// Control: the SAME attack on the owned vault is accepted in BOTH modes, so the dry run is not
/// just rejecting everything.
#[test]
fn dry_run_does_not_bypass_ownership_on_a_template_swap() {
    let mut test = new_test();
    let (wallet, alice) = funded_burn_wallet(&mut test);
    let owned = deploy(&mut test, "OwnedVault");
    deposit(&mut test, owned, &alice, 500 * TARI);
    let rug = test.get_template_address("Rug");

    // The attack, rebuilt fresh each run (try_execute consumes the transaction).
    let build_attack = |test: &TemplateTest, target: ComponentAddress| {
        tx(test)
            .update_component_template(target, rug)
            .call_method(target, "withdraw", args![Amount::from_u64(500 * TARI)])
            .put_last_instruction_output_on_workspace("loot")
            .call_method(alice.account, "deposit", args![Workspace("loot")])
            .build_and_seal(test.secret_key())
    };

    for dry in [true, false] {
        test.set_dry_run(dry);

        // Burn wallet: refused for want of an owner, dry or real.
        let t = build_attack(&test, wallet);
        let proof = test.owner_proof();
        let result = test.try_execute(t, vec![proof]).unwrap();
        assert!(!result.finalize.result.is_accept(), "burn-wallet swap ACCEPTED (dry_run={dry})");
        let reason = format!("{:?}", result.finalize.result);
        assert!(reason.contains(OWNER_REQUIRED), "burn wallet, dry_run={dry}: unexpected reason: {reason}");

        // Control: the owned vault IS swap-and-drained, dry or real — the attack is genuine.
        let t = build_attack(&test, owned);
        let proof = test.owner_proof();
        let result = test.try_execute(t, vec![proof]).unwrap();
        assert!(result.finalize.result.is_accept(), "owned-vault control REJECTED (dry_run={dry})");
    }

    // Nothing was committed by try_execute; the burn wallet is intact.
    test.set_dry_run(false);
    assert_eq!(balance(&mut test, wallet), Amount::from_u64(500 * TARI));
    assert_eq!(total(&mut test, wallet), Amount::from_u64(500 * TARI));
}

/// A dry run of a legitimate deposit produces exactly the same accept + vault delta as the real one,
/// and (via try_execute) commits nothing on its own — so dry-running a deposit to preview it is
/// faithful and side-effect-free. Confirms the safety assumption for the HONEST case.
#[test]
fn dry_run_of_a_deposit_is_a_faithful_side_effect_free_preview() {
    let mut test = new_test();
    let wallet = deploy(&mut test, "CaravelBurnWallet");
    let alice = actor(&mut test);

    let build = |test: &TemplateTest| {
        tx(test)
            .call_method(alice.account, "withdraw", args![TARI_TOKEN, Amount::from_u64(42 * TARI)])
            .put_last_instruction_output_on_workspace("funds")
            .call_method(wallet, "deposit", args![Workspace("funds")])
            .build_and_seal(&alice.secret)
    };

    test.set_dry_run(true);
    let dry = test.try_execute(build(&test), vec![alice.proof.clone()]).unwrap();
    assert!(dry.finalize.result.is_accept(), "dry-run deposit should accept");
    // The preview reports the deposit event and the vault change, but state is NOT committed.
    let dry_deposits = dry.finalize.events.iter().filter(|e| e.topic().ends_with(".Deposit")).count();
    assert_eq!(dry_deposits, 1);
    assert_eq!(balance(&mut test, wallet), Amount::zero(), "a dry run must not mutate the live vault");

    // The real submit does the same thing and commits.
    test.set_dry_run(false);
    deposit(&mut test, wallet, &alice, 42 * TARI);
    assert_eq!(balance(&mut test, wallet), Amount::from_u64(42 * TARI));
    assert_eq!(total(&mut test, wallet), Amount::from_u64(42 * TARI));
}

// ── 2. The 0.45 migrate-on-swap path ─────────────────────────────────────────

/// 0.45 gave `UpdateComponentTemplate` an optional `#[migration]` function that runs on the NEW
/// template with the migrated component's state in scope — a new way to re-instantiate an existing
/// component onto other code AND run logic that keeps/moves its vault. It is still gated on
/// component OWNERSHIP (engine processor.rs::update_component_template -> check_component_ownership),
/// so the ownerless burn wallet must reject it. Control: the owned vault is migrated and drained.
#[test]
fn migrate_on_swap_cannot_rug_the_ownerless_wallet() {
    let mut test = new_test();
    let (wallet, alice) = funded_burn_wallet(&mut test);
    let owned = deploy(&mut test, "OwnedVault");
    deposit(&mut test, owned, &alice, 500 * TARI);
    let migrate_tmpl = test.get_template_address("RuntimeMigrate");

    // Control: deployer owns `owned`, so migrate-then-withdraw drains it.
    let t = tx(&test)
        .update_component_template_address_with_migrate(owned, migrate_tmpl, "migrate", args![])
        .call_method(owned, "withdraw", args![Amount::from_u64(500 * TARI)])
        .put_last_instruction_output_on_workspace("loot")
        .call_method(alice.account, "deposit", args![Workspace("loot")]);
    let reason = as_deployer(&mut test, t);
    assert_eq!(reason, "", "control should be migrate-ruggable");
    assert_eq!(balance(&mut test, owned), Amount::zero());

    // Burn wallet: the same migrate-then-withdraw is refused for want of an owner.
    let t = tx(&test)
        .update_component_template_address_with_migrate(wallet, migrate_tmpl, "migrate", args![])
        .call_method(wallet, "withdraw", args![Amount::from_u64(500 * TARI)])
        .put_last_instruction_output_on_workspace("loot")
        .call_method(alice.account, "deposit", args![Workspace("loot")]);
    let reason = as_deployer(&mut test, t);
    assert!(reason.contains(OWNER_REQUIRED), "burn wallet migrate: unexpected reason: {reason}");

    // And a bare migrate (no withdraw) is refused too — the swap itself never takes effect.
    let t = tx(&test).update_component_template_address_with_migrate(wallet, migrate_tmpl, "migrate", args![]);
    let reason = as_deployer(&mut test, t);
    assert!(reason.contains(OWNER_REQUIRED), "burn wallet bare migrate: unexpected reason: {reason}");

    // Still the burn wallet template, still holding everything.
    let component = test.read_only_state_store().get_component(wallet).unwrap();
    assert_eq!(component.template_address(), &test.get_template_address("CaravelBurnWallet"));
    assert_eq!(balance(&mut test, wallet), Amount::from_u64(500 * TARI));
    assert_eq!(total(&mut test, wallet), Amount::from_u64(500 * TARI));
}

// ── 3. Atomic abort: a deposit that is not finalized must roll back ───────────

/// If a transaction deposits and then a LATER instruction fails, the whole transaction must abort
/// and the deposit must roll back — no half-applied deposit, no phantom balance. (It also means a
/// depositor cannot "deposit then abort" to leave a misleading balance behind.) Control: the same
/// deposit without the failing tail commits normally.
#[test]
fn a_deposit_rolls_back_cleanly_when_a_later_instruction_aborts() {
    let mut test = new_test();
    let wallet = deploy(&mut test, "CaravelBurnWallet");
    let alice = actor(&mut test);

    // deposit 500, then call the DenyAll `withdraw` on the wallet — the tail fails, so the whole tx
    // (deposit included) is rejected.
    let t = tx(&test)
        .call_method(alice.account, "withdraw", args![TARI_TOKEN, Amount::from_u64(500 * TARI)])
        .put_last_instruction_output_on_workspace("funds")
        .call_method(wallet, "deposit", args![Workspace("funds")])
        .call_method(wallet, "withdraw", args![Amount::from_u64(500 * TARI)]);
    let reason = as_actor(&mut test, &alice, t);
    assert!(!reason.is_empty(), "the aborting transaction should be rejected");
    assert_eq!(balance(&mut test, wallet), Amount::zero(), "the deposit must have rolled back");
    assert_eq!(total(&mut test, wallet), Amount::zero());

    // Control: the deposit alone commits.
    deposit(&mut test, wallet, &alice, 500 * TARI);
    assert_eq!(balance(&mut test, wallet), Amount::from_u64(500 * TARI));
}

// ── 4. Workspace / bucket handling ───────────────────────────────────────────

/// Instruction-ordering trick: put a withdrawn bucket on the workspace and try to deposit the SAME
/// slot twice — spending one bucket as if it were two. The second reference to a consumed bucket
/// must fail, aborting the whole transaction, so no double value is created and the deposit that
/// "worked" is rolled back with it. Control: depositing the slot once works.
#[test]
fn a_workspace_bucket_cannot_be_deposited_twice() {
    let mut test = new_test();
    let wallet = deploy(&mut test, "CaravelBurnWallet");
    let alice = actor(&mut test);

    let t = tx(&test)
        .call_method(alice.account, "withdraw", args![TARI_TOKEN, Amount::from_u64(10 * TARI)])
        .put_last_instruction_output_on_workspace("funds")
        .call_method(wallet, "deposit", args![Workspace("funds")])
        .call_method(wallet, "deposit", args![Workspace("funds")]);
    let reason = as_actor(&mut test, &alice, t);
    assert!(!reason.is_empty(), "double-spending the workspace bucket should be rejected");
    assert_eq!(balance(&mut test, wallet), Amount::zero(), "nothing should have been deposited");
    assert_eq!(total(&mut test, wallet), Amount::zero());

    // Control: one deposit of the slot is fine.
    deposit(&mut test, wallet, &alice, 10 * TARI);
    assert_eq!(balance(&mut test, wallet), Amount::from_u64(10 * TARI));
}

/// Splitting a bucket with `TakeFromBucket` and depositing both halves must conserve value — the
/// burn wallet must end up with exactly what was withdrawn (10), never inflated (14). Guards against
/// a processor bug where the taken-from bucket keeps its original amount.
#[test]
fn splitting_a_bucket_conserves_value() {
    let mut test = new_test();
    let wallet = deploy(&mut test, "CaravelBurnWallet");
    let alice = actor(&mut test);

    let t = tx(&test)
        .call_method(alice.account, "withdraw", args![TARI_TOKEN, Amount::from_u64(10 * TARI)])
        .put_last_instruction_output_on_workspace("funds")
        .take_from_bucket("funds", Amount::from_u64(4 * TARI), "part")
        .call_method(wallet, "deposit", args![Workspace("part")])
        .call_method(wallet, "deposit", args![Workspace("funds")]);
    let result = test.execute_expect_success(t.build_and_seal(&alice.secret), vec![alice.proof.clone()]);
    let deposits: Vec<_> = result.finalize.events.iter().filter(|e| e.topic().ends_with(".Deposit")).collect();
    assert_eq!(deposits.len(), 2);
    assert_eq!(balance(&mut test, wallet), Amount::from_u64(10 * TARI), "value must be conserved, not inflated");
    assert_eq!(total(&mut test, wallet), Amount::from_u64(10 * TARI));
}

// ── 5. Griefing: a flood of instructions ─────────────────────────────────────

/// A transaction with a large instruction count (here 60 extra reads around one deposit) must still
/// deposit exactly once and leave consistent state — the instruction loop does not corrupt or
/// double-count. (On the live network such a transaction is bounded by fees; fees are off here.)
#[test]
fn an_instruction_flood_does_not_corrupt_state() {
    let mut test = new_test();
    let wallet = deploy(&mut test, "CaravelBurnWallet");
    let alice = actor(&mut test);

    let mut b = tx(&test)
        .call_method(alice.account, "withdraw", args![TARI_TOKEN, Amount::from_u64(5 * TARI)])
        .put_last_instruction_output_on_workspace("funds")
        .call_method(wallet, "deposit", args![Workspace("funds")]);
    for _ in 0..60 {
        b = b.call_method(wallet, "balance", args![]);
    }
    let result = test.execute_expect_success(b.build_and_seal(&alice.secret), vec![alice.proof.clone()]);
    let deposits = result.finalize.events.iter().filter(|e| e.topic().ends_with(".Deposit")).count();
    assert_eq!(deposits, 1, "exactly one deposit despite the flood");
    assert_eq!(balance(&mut test, wallet), Amount::from_u64(5 * TARI));
    assert_eq!(total(&mut test, wallet), Amount::from_u64(5 * TARI));
}

// ── 6. Referencing the burn vault as a direct transaction input ───────────────

/// The burn wallet's vault id, handed as an argument and reached via `Vault::for_test`, is outside
/// any caller's component scope: a plain function has no component context, and a component may only
/// operate vaults its own state owns. Both are refused and the balance is untouched. (This repeats
/// round 1's result from the runtime angle, pinned to the engine's scope/ownership errors, as a
/// regression guard for the version under test — including the on-chain binary.)
#[test]
fn the_vault_cannot_be_driven_as_a_foreign_input() {
    let mut test = new_test();
    let (wallet, alice) = funded_burn_wallet(&mut test);
    let v = vault_id(&test, wallet);
    let helpers = test.get_template_address("TestHelpers");
    let thief: ComponentAddress = test.call_function("TestHelpers", "new", args![], vec![]);
    let one = Amount::from_u64(TARI);

    // From a plain function: no component context.
    let t = tx(&test)
        .call_function(helpers, "steal", args![v, one])
        .put_last_instruction_output_on_workspace("loot")
        .call_method(alice.account, "deposit", args![Workspace("loot")]);
    let reason = as_deployer(&mut test, t);
    assert!(reason.contains("can only be called from within a component context"), "function steal: {reason}");

    // From inside an attacker component: past the context check, stopped by vault ownership.
    let t = tx(&test)
        .call_method(thief, "steal_from", args![v, one])
        .put_last_instruction_output_on_workspace("loot")
        .call_method(alice.account, "deposit", args![Workspace("loot")]);
    let reason = as_deployer(&mut test, t);
    assert!(reason.contains(&format!("{v} is not owned by {thief}")), "component steal: {reason}");

    assert_eq!(balance(&mut test, wallet), Amount::from_u64(500 * TARI));
    assert_eq!(total(&mut test, wallet), Amount::from_u64(500 * TARI));
}
