//! TEST ONLY. Never published.
//!
//! `mint` makes a fungible token that is not TARI, for the non-TARI deposit test. `steal` and
//! `recall` attack the burn wallet's vault from OUTSIDE the burn wallet — the two ways funds could
//! leave a vault without one of its own methods. Both must fail.

use tari_template_lib::prelude::*;

#[template]
mod test_helpers {
    use super::*;

    pub struct TestHelpers {}

    impl TestHelpers {
        /// A bucket of `amount` units of a brand-new fungible resource.
        pub fn mint(amount: Amount) -> Bucket {
            ResourceBuilder::public_fungible().with_token_symbol("OTHER").initial_supply(amount)
        }

        /// Withdraw straight from someone else's vault, by id — from a plain function call.
        pub fn steal(vault_id: VaultId, amount: Amount) -> Bucket {
            Vault::for_test(vault_id).withdraw(amount)
        }

        /// An attacker component, so the next attack runs INSIDE a component context.
        pub fn new() -> Component<Self> {
            Component::new(Self {}).with_access_rules(ComponentAccessRules::allow_all()).create()
        }

        /// The same withdraw, from inside this (attacker) component's own method.
        pub fn steal_from(&self, vault_id: VaultId, amount: Amount) -> Bucket {
            Vault::for_test(vault_id).withdraw(amount)
        }

        /// Recall TARI out of someone else's vault through the resource itself.
        pub fn recall(vault_id: VaultId, amount: Amount) -> Bucket {
            ResourceManager::get(TARI_TOKEN).recall_fungible_amount(vault_id, amount)
        }
    }
}
