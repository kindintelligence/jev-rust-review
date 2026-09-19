use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;

pub async fn run(mut stream: TcpStream) -> std::io::Result<()> {
    let mut header = [0u8; 8];
    stream.read_exact(&mut header).await?;
    handle(&header);
    Ok(())
}

fn handle(frame: &[u8; 8]) {
    let _ = frame;
}
