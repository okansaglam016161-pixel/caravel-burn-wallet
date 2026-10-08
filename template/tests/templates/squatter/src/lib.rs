//! TEST ONLY. Never published.
//!
//! Tries to take ownership of another component's vault by writing its id into this component's own
//! state, then withdraw from it as its "owner". Both creating such a component and adopting the
//! vault into an existing one must fail.

use tari_template_lib::prelude::*;

#[template]
mod squatter {
    use super::*;

    pub struct Squatter {
        vault: Option<Vault>,
    }

    impl Squatter {
        /// Create a component whose state holds someone else's vault.
        pub fn new_holding(vault_id: VaultId) -> Component<Self> {
            Component::new(Self { vault: Some(Vault::for_test(vault_id)) })
                .with_access_rules(ComponentAccessRules::allow_all())
                .create()
        }

        /// An empty squatter, to adopt a vault into afterwards.
        pub fn new() -> Component<Self> {
            Component::new(Self { vault: None }).with_access_rules(ComponentAccessRules::allow_all()).create()
        }

        /// Write someone else's vault into this component's state, then withdraw from it.
        pub fn adopt_and_withdraw(&mut self, vault_id: VaultId, amount: Amount) -> Bucket {
            self.vault = Some(Vault::for_test(vault_id));
            self.vault.as_ref().unwrap().withdraw(amount)
        }

        /// Just write it into state (no withdraw in the same call).
        pub fn adopt(&mut self, vault_id: VaultId) {
            self.vault = Some(Vault::for_test(vault_id));
        }
    }
}
