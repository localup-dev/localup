//! TCP proxy implementation

use tokio::net::TcpStream;

/// TCP proxy for bidirectional data forwarding
pub struct TcpProxy;

impl TcpProxy {
    /// Proxy data between two TCP streams
    pub async fn proxy(mut stream1: TcpStream, mut stream2: TcpStream) -> std::io::Result<()> {
        let (mut r1, mut w1) = stream1.split();
        let (mut r2, mut w2) = stream2.split();

        // Spawn two tasks for bidirectional forwarding
        let forward1 = tokio::io::copy(&mut r1, &mut w2);
        let forward2 = tokio::io::copy(&mut r2, &mut w1);

        // Wait for both directions to complete
        tokio::try_join!(forward1, forward2)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tcp_proxy_struct() {
        let _ = TcpProxy;
    }
}
