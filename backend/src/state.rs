//! Shared application state. (Job tracking is added in Phase 1.)

#[derive(Clone, Default)]
pub struct AppState {}

impl AppState {
    pub fn new() -> Self {
        AppState::default()
    }
}
