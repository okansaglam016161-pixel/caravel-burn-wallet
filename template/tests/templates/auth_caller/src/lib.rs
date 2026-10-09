//! TEST ONLY. Never published.
//!
//! A component that makes cross-template method calls into another component (the burn wallet), to
//! probe WHOSE authorization context a nested call runs in. A method's access rule is evaluated
//! against the callee's own frame (the actor is the top frame), and the burn wallet has no owner,
//! so a caller's identity/badges never let it reach a `deny_all` method. Depositing through this
//! component works (deposit is `allow_all`) and is still counted; reaching any other method fails.

use tari_template_lib::prelude::*;

#[template]
mod auth_caller {
    use super::*;

    pub struct Caller {}

    impl Caller {
        pub fn new() -> Component<Self> {
            Component::new(Self {}).with_access_rules(AccessRules::allow_all()).create()
        }

        /// Forward a bucket into `target`'s `deposit` from inside this component's frame (control: works).
        pub fn deposit_into(&self, target: ComponentAddress, bucket: Bucket) {
            ComponentManager::get(target).invoke("deposit", args![bucket]);
        }

        /// Try to reach a `deny_all` / non-existent method on `target` from a nested call context.
        pub fn call_method_on(&self, target: ComponentAddress, method: String, amount: Amount) {
            ComponentManager::get(target).invoke(method, args![amount]);
        }
    }
}
