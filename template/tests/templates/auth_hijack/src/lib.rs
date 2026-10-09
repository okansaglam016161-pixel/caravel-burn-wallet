//! TEST ONLY. Never published.
//!
//! Tries to seize another component (the burn wallet) from OUTSIDE it, via the engine's
//! component ops that take an explicit target address: `ComponentManager::get(target)` then
//! `set_access_rules` / `set_owner_rule` / `set_state`. All three must fail, because the engine
//! operates each op on the CURRENTLY EXECUTING component's write lock (this attacker component),
//! and refuses when the named target address is not that locked component:
//!   - set_access_rules / set_owner_rule -> LockError::SubstateNotLocked(target)
//!   - set_state                         -> AccessDeniedSetComponentState
//! The same ops on the attacker's OWN component (target = self) succeed — the control.

use tari_template_lib::prelude::*;
use tari_template_lib::types::SubstateOwnerRule;

#[template]
mod auth_hijack {
    use super::*;

    pub struct Hijack {
        marker: Amount,
    }

    impl Hijack {
        /// Owner defaults to the deployer; access is open so anyone can drive the attacks.
        pub fn new() -> Component<Self> {
            Component::new(Self { marker: Amount::zero() })
                .with_access_rules(AccessRules::allow_all())
                .create()
        }

        /// Open `target`'s method rules to everyone (would let anyone withdraw once a withdraw method existed).
        pub fn open_access_rules_on(&mut self, target: ComponentAddress) {
            ComponentManager::get(target).set_access_rules(AccessRules::allow_all());
        }

        /// Make "anyone" the owner of `target` (an owner may change every rule and call every method).
        pub fn seize_owner_rule_on(&mut self, target: ComponentAddress) {
            ComponentManager::get(target).set_owner_rule(SubstateOwnerRule::ByAccessRule(AccessRule::AllowAll));
        }

        /// Overwrite `target`'s state (e.g. to fake a balance / deposit history).
        pub fn overwrite_state_on(&mut self, target: ComponentAddress, value: Amount) {
            ComponentManager::get(target).set_state(value);
        }

        // ── Controls: the same ops on this component itself must work ────────

        pub fn open_access_rules_on_self(&mut self) {
            ComponentManager::current().set_access_rules(AccessRules::allow_all());
        }

        pub fn overwrite_own_state(&mut self, value: Amount) {
            self.marker = value;
        }
    }
}
