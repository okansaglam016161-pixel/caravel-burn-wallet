//! TEST ONLY. Never published.
//!
//! The README's "use it from another contract" example: a shop that sells for TARI and burns 2% of
//! every sale into the burn wallet by calling its `deposit(bucket)`.

use tari_template_lib::prelude::*;

#[template]
mod shop {
    use super::*;

    /// The share of every sale that is burned, in percent.
    const BURN_PERCENT: u128 = 2;

    pub struct Shop {
        /// The official burn wallet: component_2e91fb78…6029 on esmeralda.
        burn_wallet: ComponentAddress,
        price: Amount,
        till: Vault,
    }

    impl Shop {
        pub fn new(burn_wallet: ComponentAddress, price: Amount) -> Component<Self> {
            Component::new(Self { burn_wallet, price, till: Vault::new_empty(TARI_TOKEN) })
                .with_access_rules(ComponentAccessRules::new().add_method_rule("buy", rule!(allow_all)))
                .create()
        }

        pub fn buy(&mut self, mut payment: Bucket) {
            assert!(payment.resource_address() == TARI_TOKEN, "Pay in TARI");
            assert!(payment.amount() == self.price, "Pay exactly the price");

            let burn = Amount::new(self.price.to_u128() * BURN_PERCENT / 100);
            // The burn wallet refuses an empty deposit (and fails the whole sale), so skip it when
            // the share rounds down to zero.
            if burn.is_positive() {
                ComponentManager::get(self.burn_wallet).invoke("deposit", args![payment.take(burn)]);
            }
            self.till.deposit(payment);
        }

        pub fn till(&self) -> Amount {
            self.till.balance()
        }
    }
}
