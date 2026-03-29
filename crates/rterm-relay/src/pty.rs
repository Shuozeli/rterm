use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{Read, Write};
use tokio::sync::mpsc;
use tracing::{debug, error};

/// A PTY session: spawns a shell and provides channels for I/O.
pub struct PtySession {
    /// Send bytes to PTY stdin.
    pub stdin_tx: mpsc::Sender<Vec<u8>>,
    /// Receive bytes from PTY stdout.
    pub stdout_rx: mpsc::Receiver<Vec<u8>>,
    /// Send resize events (cols, rows).
    pub resize_tx: mpsc::Sender<(u16, u16)>,
}

impl PtySession {
    /// Spawn a new PTY session with the given shell and initial size.
    pub fn spawn(
        shell: &str,
        cols: u16,
        rows: u16,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let mut cmd = CommandBuilder::new(shell);
        cmd.env("TERM", "xterm-256color");
        let _child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);

        let master = pair.master;

        // Take writer and reader from master before moving it.
        let mut writer = master.take_writer()?;
        let mut reader = master.try_clone_reader()?;

        // Stdin channel: send bytes to PTY.
        let (stdin_tx, mut stdin_rx) = mpsc::channel::<Vec<u8>>(64);
        std::thread::spawn(move || {
            while let Some(data) = stdin_rx.blocking_recv() {
                if writer.write_all(&data).is_err() {
                    break;
                }
            }
            debug!("PTY stdin writer thread exited");
        });

        // Stdout channel: read bytes from PTY.
        let (stdout_tx, stdout_rx) = mpsc::channel::<Vec<u8>>(64);
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if stdout_tx.blocking_send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        debug!("PTY read error: {}", e);
                        break;
                    }
                }
            }
            debug!("PTY stdout reader thread exited");
        });

        // Resize channel.
        let (resize_tx, mut resize_rx) = mpsc::channel::<(u16, u16)>(8);
        // Master needs to be kept alive for resize. Move it into the resize task.
        tokio::spawn(async move {
            while let Some((cols, rows)) = resize_rx.recv().await {
                if let Err(e) = master.resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                }) {
                    error!("PTY resize error: {}", e);
                }
            }
            debug!("PTY resize task exited, master dropped");
            // master is dropped here, which closes the PTY.
        });

        Ok(PtySession {
            stdin_tx,
            stdout_rx,
            resize_tx,
        })
    }
}
