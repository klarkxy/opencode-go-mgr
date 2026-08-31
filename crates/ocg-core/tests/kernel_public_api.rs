//! External-crate smoke for the Stage 1 kernel facade.
//!
//! `PricingSnapshot::estimate` must remain an inherent method so dependents
//! can call `embedded_seed().estimate(...)` without importing a helper trait.

use ocg_core::pricing::embedded_seed;

#[test]
fn embedded_seed_estimate_is_an_inherent_method() {
    let estimate = embedded_seed().estimate("glm-5.3", 1, 1, 0, 0, None);
    assert_eq!(estimate.cost_state, "priced");
    assert!(estimate.cost.is_some());
}
