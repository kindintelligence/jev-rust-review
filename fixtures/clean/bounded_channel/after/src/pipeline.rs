use tokio::sync::mpsc;

pub struct Job {
    pub id: u64,
}

pub async fn run(jobs: Vec<Job>) -> u64 {
    let (tx, mut rx) = mpsc::channel::<Job>(64);
    let worker = tokio::spawn(async move {
        let mut done = 0;
        while let Some(job) = rx.recv().await {
            done += job.id.min(1);
        }
        done
    });
    for job in jobs {
        if tx.send(job).await.is_err() {
            break;
        }
    }
    drop(tx);
    worker.await.unwrap_or(0)
}
