use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio::time::{interval, Duration};

pub async fn run(mut stream: TcpStream) -> std::io::Result<()> {
    let mut header = [0u8; 8];
    let mut tick = interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            res = stream.read_exact(&mut header) => {
                res?;
                handle(&header);
            }
            _ = tick.tick() => {
                send_ping();
            }
        }
    }
}

fn handle(frame: &[u8; 8]) {
    let _ = frame;
}

fn send_ping() {}
