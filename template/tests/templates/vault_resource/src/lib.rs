//! TEST ONLY. Never published.
//!
//! Attacks through the TARI `ResourceManager`, aimed at the burn wallet's guarantees that nothing
//! can be minted, that funds can't be destroyed from under it, and that TARI stays depositable
//! forever. `mint_tari` tries to create TARI out of nothing; `burn_tari` tries to destroy a bucket
//! of TARI; `set_tari_rule` tries to change one of TARI's access rules (e.g. flip Deposit to
//! deny_all, which would stop every future deposit network-wide). On the live network TARI's mint,
//! burn, recall and freeze rules are DenyAll and every updater is Locked, so all of these must be
//! refused by the engine.

use tari_template_lib::prelude::*;

#[template]
mod vault_resource {
    use super::*;

    pub struct VaultResource {}

    impl VaultResource {
        /// Mint `amount` of brand-new TARI (gated by TARI's Mint rule = DenyAll).
        pub fn mint_tari(amount: Amount) -> Bucket {
            ResourceManager::get(TARI_TOKEN).mint_fungible(amount)
        }

        /// Destroy a bucket of TARI (gated by TARI's Burn rule = DenyAll).
        pub fn burn_tari(bucket: Bucket) {
            bucket.burn();
        }

        /// Change one of TARI's access rules. `allow` picks allow_all vs deny_all. Gated by the
        /// matching `*_updater`, all of which are Locked on TARI.
        pub fn set_tari_rule(action: ResourceAuthAction, allow: bool) {
            let new_rule = if allow { rule!(allow_all) } else { rule!(deny_all) };
            ResourceManager::get(TARI_TOKEN).update_access_rule(action, new_rule);
        }
    }
}
