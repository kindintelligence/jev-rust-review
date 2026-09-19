use std::sync::Arc;

pub struct State {
    pub name: String,
}

pub async fn start(state: Arc<State>, workers: usize) {
    let _ = (state, workers);
}
