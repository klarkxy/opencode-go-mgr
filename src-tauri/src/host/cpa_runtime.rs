//! Installed-desktop CPA child lifecycle registration.
use ocg_core::state::CoreState;

pub fn register(core: &CoreState) {
    ocg_core::cpa_runtime::host::register_owned_host(core);
}

pub fn stop_on_exit(core: &CoreState) {
    core.stop_owned_cpa_runtime();
}
