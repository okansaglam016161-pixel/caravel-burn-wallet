//! TEST ONLY. Never published.
//!
//! The code an attacker would swap a vault component onto with `UpdateComponentTemplate`: the burn
//! wallet's state layout (vault, total_deposited — the engine keeps the component's state and only
//! changes which template runs it) plus a withdraw that hands the TARI to the caller.

use tari_template_lib::prelude::*;

#[template]
mod rug {
    use super::*;

    pub struct Rug {
        vault: Vault,
        total_deposited: Amount,
    }

    impl Rug {
        pub fn withdraw(&mut self, amount: Amount) -> Bucket {
            self.vault.withdraw(amount)
        }

        pub fn balance(&self) -> Amount {
            self.vault.balance()
        }
    }
}
