//! TEST ONLY. Never published.
//!
//! Fee- and proof-based attacks on the burn wallet's vault that run INSIDE a component context (so
//! the engine's `check_component_scope` is the only thing standing between the attacker and the
//! funds). Each attack takes a foreign vault id and tries to pay this transaction's fee out of it,
//! lock its funds with a proof, or do a confidential withdraw from it. The component also owns its
//! OWN TARI vault, which the test uses as a CONTROL: the identical move succeeds against the vault
//! this component actually owns, so a refusal against the burn wallet's vault is the scope check,
//! not a broken call. Every foreign-vault attack must fail and leave the burn wallet untouched.

use tari_template_lib::prelude::*;

#[template]
mod vault_feethief {
    use super::*;

    pub struct VaultFeethief {
        /// A vault this component genuinely owns, for the control moves.
        own: Vault,
    }

    impl VaultFeethief {
        pub fn new() -> Component<Self> {
            Component::new(Self { own: Vault::new_empty(TARI_TOKEN) })
                .with_access_rules(ComponentAccessRules::allow_all())
                .create()
        }

        /// Fund our own vault (so the control moves have something to act on).
        pub fn fund(&mut self, bucket: Bucket) {
            self.own.deposit(bucket);
        }

        // ── CONTROLS: the same moves against the vault we own ────────────────

        /// Pay this transaction's fee out of OUR vault. Works: we own it.
        pub fn pay_fee_own(&mut self, amount: Amount) {
            self.own.pay_fee(amount);
        }

        /// Lock funds in OUR vault with a proof, then drop it. Works: we own it.
        pub fn lock_own(&mut self, amount: Amount) {
            let proof = self.own.create_proof_by_amount(amount);
            proof.drop();
        }

        // ── ATTACKS: the same moves against a FOREIGN vault id ───────────────

        /// Pay this transaction's fee out of someone else's vault.
        pub fn pay_fee_foreign(&self, vault_id: VaultId, amount: Amount) {
            Vault::for_test(vault_id).pay_fee(amount);
        }

        /// Lock someone else's vault's funds with a proof (griefing / future-deposit DoS attempt).
        pub fn lock_foreign(&self, vault_id: VaultId, amount: Amount) {
            let proof = Vault::for_test(vault_id).create_proof_by_amount(amount);
            proof.drop();
        }

        /// Confidential-withdraw from someone else's vault.
        pub fn withdraw_confidential_foreign(
            &self,
            vault_id: VaultId,
            proof: ConfidentialWithdrawProof,
        ) -> Bucket {
            Vault::for_test(vault_id).withdraw_confidential(proof)
        }
    }
}
