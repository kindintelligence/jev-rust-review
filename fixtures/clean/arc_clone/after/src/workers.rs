use std::sync::Arc;

pub struct State {
    pub name: String,
}

pub async fn start(state: Arc<State>, workers: usize) {
    let mut handles = Vec::with_capacity(workers);
    for id in 0..workers {
        let state = Arc::clone(&state);
        handles.push(tokio::spawn(async move { work(id, &state).await }));
    }
    for h in handles {
        if let Err(e) = h.await {
            eprintln!("worker failed: {e}");
        }
    }
}

async fn work(id: usize, state: &State) {
    let _ = (id, state.name.len());
}
