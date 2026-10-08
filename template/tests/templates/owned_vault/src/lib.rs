//! TEST ONLY. Never published.
//!
//! The CONTROL for the template-swap test: the burn wallet's exact state layout and public surface,
//! but created WITHOUT `.with_owner_rule(OwnerRule::None)` — so the engine's default applies and the
//! deployer owns it. The swap test shows this one CAN be rugged by its deployer, and the burn wallet
//! cannot: the only difference is the owner rule.

use tari_template_lib::prelude::*;

#[template]
mod owned_vault {
    use super::*;

    pub struct OwnedVault {
        vault: Vault,
        total_deposited: Amount,
    }

    impl OwnedVault {
        pub fn new() -> Component<Self> {
            let access_rules = ComponentAccessRules::new()
                .add_method_rule("deposit", rule!(allow_all))
                .add_method_rule("balance", rule!(allow_all))
                .default(rule!(deny_all));
            Component::new(Self { vault: Vault::new_empty(TARI_TOKEN), total_deposited: Amount::zero() })
                .with_access_rules(access_rules)
                .create()
        }

        pub fn deposit(&mut self, bucket: Bucket) {
            self.total_deposited = self.total_deposited.checked_add(bucket.amount()).unwrap();
            self.vault.deposit(bucket);
        }

        pub fn balance(&self) -> Amount {
            self.vault.balance()
        }
    }
}
