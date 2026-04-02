use std::sync::Arc;

use async_trait::async_trait;
use russh::keys::{self, PrivateKeyWithHashAlg};
use russh::{ChannelMsg, Disconnect, client};
use tokio::sync::mpsc;
use tracing::debug;

use crate::Transport;
use crate::error::TransportError;

/// SSH connection configuration.
pub struct SshConfig {
    pub hostname: String,
    pub port: u16,
    pub username: String,
    pub auth: SshAuth,
    pub cols: u16,
    pub rows: u16,
}

/// SSH authentication method.
pub enum SshAuth {
    Password(String),
    Key {
        private_key_pem: String,
        passphrase: Option<String>,
    },
}

/// Minimal client handler that accepts all host keys and forwards
/// channel data to an mpsc channel for SshTransport::read().
struct ClientHandler;

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &keys::ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        // Accept all host keys (no known_hosts verification).
        Ok(true)
    }
}

/// SSH transport implementing the `Transport` trait.
///
/// Uses russh to maintain an SSH session with a PTY channel.
/// Data is read via the channel's `wait()` loop running in a
/// background task that forwards `ChannelMsg::Data` to an mpsc channel.
pub struct SshTransport {
    /// Receives stdout data forwarded by the background reader task.
    data_rx: mpsc::Receiver<Vec<u8>>,
    /// Write half of the SSH channel (for sending data + window changes).
    write_half: russh::ChannelWriteHalf<client::Msg>,
    /// Connection handle for disconnect.
    handle: client::Handle<ClientHandler>,
}

impl SshTransport {
    /// Connect to an SSH server, authenticate, open a PTY session,
    /// and return an `SshTransport` ready for read/write.
    pub async fn connect(config: SshConfig) -> Result<Self, TransportError> {
        let client_config = client::Config::default();
        let client_config = Arc::new(client_config);

        let addr = (config.hostname.as_str(), config.port);
        let mut handle = client::connect(client_config, addr, ClientHandler)
            .await
            .map_err(|e| TransportError::Spawn(Box::new(e)))?;

        // Authenticate
        let auth_result = match config.auth {
            SshAuth::Password(ref password) => handle
                .authenticate_password(&config.username, password)
                .await
                .map_err(|e| TransportError::Spawn(Box::new(e)))?,
            SshAuth::Key {
                ref private_key_pem,
                ref passphrase,
            } => {
                let key = keys::decode_secret_key(private_key_pem, passphrase.as_deref())
                    .map_err(|e| TransportError::Spawn(Box::new(e)))?;

                let hash_alg = handle
                    .best_supported_rsa_hash()
                    .await
                    .map_err(|e| TransportError::Spawn(Box::new(e)))?
                    .flatten();

                let key_with_alg = PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg);

                handle
                    .authenticate_publickey(&config.username, key_with_alg)
                    .await
                    .map_err(|e| TransportError::Spawn(Box::new(e)))?
            }
        };

        if !auth_result.success() {
            return Err(TransportError::Spawn("SSH authentication failed".into()));
        }

        // Open session channel
        let channel = handle
            .channel_open_session()
            .await
            .map_err(|e| TransportError::Spawn(Box::new(e)))?;

        // Split into read and write halves
        let (read_half, write_half) = channel.split();

        // Request PTY
        write_half
            .request_pty(
                false,
                "xterm-256color",
                u32::from(config.cols),
                u32::from(config.rows),
                0,
                0,
                &[],
            )
            .await
            .map_err(|e| TransportError::Spawn(Box::new(e)))?;

        // Request shell
        write_half
            .request_shell(false)
            .await
            .map_err(|e| TransportError::Spawn(Box::new(e)))?;

        // Spawn a background task to read ChannelMsg and forward data.
        let (data_tx, data_rx) = mpsc::channel::<Vec<u8>>(64);
        tokio::spawn(async move {
            let mut read_half = read_half;
            loop {
                match read_half.wait().await {
                    Some(ChannelMsg::Data { data }) => {
                        if data_tx.send(data.to_vec()).await.is_err() {
                            break;
                        }
                    }
                    Some(ChannelMsg::ExtendedData { data, .. }) => {
                        // Forward stderr as regular data.
                        if data_tx.send(data.to_vec()).await.is_err() {
                            break;
                        }
                    }
                    Some(ChannelMsg::Eof | ChannelMsg::Close) => {
                        debug!("SSH channel EOF/Close");
                        break;
                    }
                    Some(_) => {
                        // Ignore other messages (ExitStatus, etc.)
                    }
                    None => {
                        debug!("SSH channel receiver closed");
                        break;
                    }
                }
            }
        });

        Ok(Self {
            data_rx,
            write_half,
            handle,
        })
    }
}

#[async_trait]
impl Transport for SshTransport {
    async fn read(&mut self) -> Result<Vec<u8>, TransportError> {
        self.data_rx.recv().await.ok_or(TransportError::Closed)
    }

    async fn write(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.write_half
            .data(data)
            .await
            .map_err(|e| TransportError::Spawn(Box::new(e)))
    }

    async fn resize(&mut self, cols: u16, rows: u16) -> Result<(), TransportError> {
        self.write_half
            .window_change(u32::from(cols), u32::from(rows), 0, 0)
            .await
            .map_err(|e| TransportError::Spawn(Box::new(e)))
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        let _ = self
            .handle
            .disconnect(Disconnect::ByApplication, "", "English")
            .await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_config_fields() {
        let config = SshConfig {
            hostname: "example.com".to_string(),
            port: 22,
            username: "user".to_string(),
            auth: SshAuth::Password("secret".to_string()),
            cols: 80,
            rows: 24,
        };
        assert_eq!(config.hostname, "example.com");
        assert_eq!(config.port, 22);
        assert_eq!(config.username, "user");
        assert_eq!(config.cols, 80);
        assert_eq!(config.rows, 24);
        assert!(matches!(config.auth, SshAuth::Password(ref p) if p == "secret"));
    }

    #[test]
    fn ssh_config_key_auth() {
        let config = SshConfig {
            hostname: "10.0.0.1".to_string(),
            port: 2222,
            username: "admin".to_string(),
            auth: SshAuth::Key {
                private_key_pem:
                    "-----BEGIN OPENSSH PRIVATE KEY-----\ntest\n-----END OPENSSH PRIVATE KEY-----"
                        .to_string(),
                passphrase: Some("pass".to_string()),
            },
            cols: 120,
            rows: 40,
        };
        assert_eq!(config.port, 2222);
        assert!(
            matches!(config.auth, SshAuth::Key { ref passphrase, .. } if passphrase.as_deref() == Some("pass"))
        );
    }

    #[test]
    fn ssh_transport_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<SshTransport>();
    }

    /// Integration test: connect to an in-process mock SSH server, send data, read it back.
    ///
    /// This uses russh's server API to spin up an ephemeral SSH server that echos
    /// data back to the client, then connects SshTransport to it.
    #[tokio::test]
    async fn ssh_transport_echo_via_mock_server() {
        use russh::keys::ssh_key::PrivateKey;
        use russh::keys::ssh_key::rand_core::OsRng;
        use russh::server::{self as srv, Auth, Server as _};
        use russh::{Channel, ChannelId, Pty};
        use std::collections::HashMap;
        use std::sync::Mutex;

        // -- Mock server handler --
        #[derive(Clone)]
        struct MockServer {
            clients: Arc<Mutex<HashMap<(usize, ChannelId), srv::Handle>>>,
            id: usize,
        }

        impl srv::Server for MockServer {
            type Handler = MockServer;
            fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
                let s = self.clone();
                self.id += 1;
                s
            }
        }

        impl srv::Handler for MockServer {
            type Error = russh::Error;

            async fn channel_open_session(
                &mut self,
                channel: Channel<srv::Msg>,
                session: &mut srv::Session,
            ) -> Result<bool, Self::Error> {
                self.clients
                    .lock()
                    .unwrap()
                    .insert((self.id, channel.id()), session.handle());
                Ok(true)
            }

            async fn auth_password(
                &mut self,
                _user: &str,
                _password: &str,
            ) -> Result<Auth, Self::Error> {
                Ok(Auth::Accept)
            }

            async fn pty_request(
                &mut self,
                _channel: ChannelId,
                _term: &str,
                _col_width: u32,
                _row_height: u32,
                _pix_width: u32,
                _pix_height: u32,
                _modes: &[(Pty, u32)],
                session: &mut srv::Session,
            ) -> Result<(), Self::Error> {
                session.channel_success(_channel)?;
                Ok(())
            }

            async fn shell_request(
                &mut self,
                channel: ChannelId,
                session: &mut srv::Session,
            ) -> Result<(), Self::Error> {
                session.channel_success(channel)?;
                Ok(())
            }

            async fn data(
                &mut self,
                channel: ChannelId,
                data: &[u8],
                session: &mut srv::Session,
            ) -> Result<(), Self::Error> {
                // Echo data back to client.
                session.data(channel, data.to_vec())?;
                Ok(())
            }
        }

        // -- Start mock server --
        let server_key =
            PrivateKey::random(&mut OsRng, russh::keys::ssh_key::Algorithm::Ed25519).unwrap();
        let mut server_config = srv::Config::default();
        server_config.keys.push(server_key);
        server_config.inactivity_timeout = None;
        server_config.auth_rejection_time = std::time::Duration::from_secs(1);
        let server_config = Arc::new(server_config);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let srv_config = server_config.clone();
        tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut sh = MockServer {
                clients: Arc::new(Mutex::new(HashMap::new())),
                id: 0,
            };
            let handler = sh.new_client(socket.peer_addr().ok());
            let _ = srv::run_stream(srv_config, socket, handler).await;
        });

        // -- Connect SshTransport --
        let ssh_config = SshConfig {
            hostname: addr.ip().to_string(),
            port: addr.port(),
            username: "testuser".to_string(),
            auth: SshAuth::Password("testpass".to_string()),
            cols: 80,
            rows: 24,
        };

        let mut transport = SshTransport::connect(ssh_config).await.unwrap();

        // Write data and read it back (echo server).
        transport.write(b"hello ssh").await.unwrap();
        let received = transport.read().await.unwrap();
        assert_eq!(received, b"hello ssh");

        // Test resize (should not error).
        transport.resize(120, 40).await.unwrap();

        // Close transport.
        transport.close().await.unwrap();
    }
}
