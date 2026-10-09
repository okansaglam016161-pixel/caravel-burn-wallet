//! TEST ONLY. Never published.
//!
//! The code an attacker would swap a vault component onto with `UpdateComponentTemplate` **with a
//! `#[migration]` function set** — the variant round 1 did not cover (round 1 only tried the
//! no-migrate swap). The migration keeps the existing vault (so no substate is orphaned) and the
//! template then exposes a `withdraw` that drains it. The attack is gated on component OWNERSHIP in
//! exactly the same place as the no-migrate swap (`check_component_ownership` runs after the
//! migration frame is pushed but before the migration function executes), so `OwnerRule::None`
//! blocks it for everyone and the migration body never runs on the burn wallet.

use tari_template_lib::prelude::*;

/// Decodes the burn wallet / owned-vault state layout `{vault, total_deposited}`. A migration
/// function's first typed argument is the previous component state, decoded by the engine.
#[derive(minicbor::Decode, minicbor::Encode, minicbor::CborLen)]
pub struct OldLayout {
    #[n(0)]
    vault: Vault,
    #[n(1)]
    total_deposited: Amount,
}

#[template]
mod auth_rug_migrate {
    use super::*;

    pub struct RugMigrate {
        vault: Vault,
        total_deposited: Amount,
    }

    impl RugMigrate {
        /// Migration: keep the vault, carry the total across. Returning the new struct makes the
        /// engine persist it as the component's state.
        #[migration]
        pub fn steal(old: OldLayout) -> Self {
            Self {
                vault: old.vault,
                total_deposited: old.total_deposited,
            }
        }

        /// After the swap, hand the caller everything in the (adopted) vault.
        pub fn withdraw(&mut self, amount: Amount) -> Bucket {
            self.vault.withdraw(amount)
        }

        pub fn balance(&self) -> Amount {
            self.vault.balance()
        }
    }
}
