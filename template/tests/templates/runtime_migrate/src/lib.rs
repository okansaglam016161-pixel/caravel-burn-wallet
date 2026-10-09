//! TEST ONLY. Never published.
//!
//! The target of the 0.45 "migrate-on-swap" path. `UpdateComponentTemplate` can now carry an
//! optional `#[migration]` function that runs on the NEW template, in a frame whose scope is the
//! migrated component's own state — so it can read (and keep, or steal) the old vault. This
//! template has the burn wallet's state layout (vault, total_deposited), a migration that
//! preserves the vault, and a `withdraw` that hands the TARI to the caller. Migrating is gated on
//! component OWNERSHIP (engine `processor.rs::update_component_template` ->
//! `check_component_ownership`), so the ownerless burn wallet must reject it; the owned control is
//! drained by it.

use tari_template_lib::prelude::*;

/// The burn wallet's (and the owned-vault control's) on-chain state layout, as the engine will
/// decode the OLD component's state when the migration runs. Field ORDER / `#[n(N)]` positions
/// must match {vault, total_deposited}.
#[derive(minicbor::Decode, minicbor::Encode, minicbor::CborLen)]
pub struct OldVaultState {
    #[n(0)]
    vault: Vault,
    #[n(1)]
    total_deposited: Amount,
}

#[template]
mod runtime_migrate {
    use super::*;

    pub struct RuntimeMigrate {
        vault: Vault,
        total_deposited: Amount,
    }

    impl RuntimeMigrate {
        /// The migration: the engine decodes the OLD component's state (same {vault,
        /// total_deposited} layout) into `old` and sets the component's new state to what we
        /// return. We keep the vault untouched so a `withdraw` can follow.
        #[migration]
        pub fn migrate(old: OldVaultState) -> Self {
            Self { vault: old.vault, total_deposited: old.total_deposited }
        }

        /// The rug: move the kept TARI out to the caller.
        pub fn withdraw(&mut self, amount: Amount) -> Bucket {
            self.vault.withdraw(amount)
        }

        pub fn balance(&self) -> Amount {
            self.vault.balance()
        }
    }
}
