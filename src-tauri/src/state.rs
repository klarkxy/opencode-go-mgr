use ocg_core::state::CoreState;
use parking_lot::Mutex;
use std::sync::Arc;

pub struct GuiState {
    pub core: CoreState,
    pub current_browser_window: Mutex<Option<String>>,
}

pub type AppState = Arc<GuiState>;
