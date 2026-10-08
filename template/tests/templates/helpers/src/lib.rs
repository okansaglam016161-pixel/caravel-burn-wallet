//! TEST ONLY. Never published.
//!
//! Tokens that are not TARI (for the refusal tests), a fake `Deposit` event, and attacks on the burn
//! wallet's vault from OUTSIDE the burn wallet — every way template code can try to take, spend,
//! freeze or recall funds held in another component's vault. All of the attacks must fail.

use tari_template_lib::prelude::*;

#[template]
mod test_helpers {
    use super::*;

    pub struct TestHelpers {}

    impl TestHelpers {
        // ── Not TARI ─────────────────────────────────────────────────────────

        /// A bucket of `amount` units of a brand-new fungible resource.
        pub fn mint(amount: Amount) -> Bucket {
            ResourceBuilder::public_fungible().with_token_symbol("OTHER").initial_supply(amount)
        }

        /// A look-alike: a new fungible resource whose symbol is `symbol` (e.g. "tTARI").
        pub fn mint_named(symbol: String, amount: Amount) -> Bucket {
            ResourceBuilder::public_fungible().with_token_symbol(symbol).initial_supply(amount)
        }

        /// One NFT of a brand-new non-fungible resource.
        pub fn mint_nft() -> Bucket {
            ResourceBuilder::non_fungible().with_token_symbol("NFT").initial_supply([NonFungibleId::from_u64(1)])
        }

        /// A `Deposit` event that is NOT from the burn wallet.
        pub fn fake_deposit(amount: Amount) {
            emit_event("Deposit", metadata!["amount" => amount.to_string()]);
        }

        // ── Attacks from a plain function (no component context) ─────────────

        /// Withdraw straight from someone else's vault, by id.
        pub fn steal(vault_id: VaultId, amount: Amount) -> Bucket {
            Vault::for_test(vault_id).withdraw(amount)
        }

        pub fn steal_all(vault_id: VaultId) -> Bucket {
            Vault::for_test(vault_id).withdraw_all()
        }

        /// Pay this transaction's fee out of someone else's vault.
        pub fn pay_fee_from(vault_id: VaultId, amount: Amount) {
            Vault::for_test(vault_id).pay_fee(amount);
        }

        /// Recall TARI out of someone else's vault through the resource itself.
        pub fn recall(vault_id: VaultId, amount: Amount) -> Bucket {
            ResourceManager::get(TARI_TOKEN).recall_fungible_amount(vault_id, amount)
        }

        pub fn recall_all(vault_id: VaultId) -> Bucket {
            ResourceManager::get(TARI_TOKEN).recall_all(vault_id)
        }

        /// Freeze someone else's vault (locking the funds in place for good).
        pub fn freeze(vault_id: VaultId) {
            ResourceManager::get(TARI_TOKEN).freeze_vault(vault_id);
        }

        /// Open TARI's recall rule to everyone, so a recall would then work.
        pub fn open_tari_recall() {
            ResourceManager::get(TARI_TOKEN).update_access_rule(ResourceAuthAction::Recall, rule!(allow_all));
        }

        // ── The same, from inside an attacker component ──────────────────────

        /// An attacker component, so these attacks run INSIDE a component context.
        pub fn new() -> Component<Self> {
            Component::new(Self {}).with_access_rules(ComponentAccessRules::allow_all()).create()
        }

        pub fn steal_from(&self, vault_id: VaultId, amount: Amount) -> Bucket {
            Vault::for_test(vault_id).withdraw(amount)
        }

        pub fn steal_all_from(&self, vault_id: VaultId) -> Bucket {
            Vault::for_test(vault_id).withdraw_all()
        }

        pub fn pay_fee_from_component(&self, vault_id: VaultId, amount: Amount) {
            Vault::for_test(vault_id).pay_fee(amount);
        }
    }
}
