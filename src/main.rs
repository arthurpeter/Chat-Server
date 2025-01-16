use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::signal::ctrl_c;
use tokio::sync::broadcast::{self, Receiver, Sender};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

#[tokio::main]
async fn main() -> Result<(), ()> {
    let listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();
    let mut handles = Vec::new();
    let (tx, _rx) = broadcast::channel(100);
    let ids = Arc::new(Mutex::new(0));

    loop {
        tokio::select! {
            _ = ctrl_c() => {
                println!("\nStarting shutdown sequence...");
                for handle in handles {
                    if let Err(err) = handle.await {
                        eprintln!("{:?}", err);
                        return Err(());
                    }
                }
                println!("Shutdown complete.");
                return Ok(())
            }
            val = listener.accept() => {
                let (socket, addr) = val.unwrap();
                let tx_clone = tx.clone();
                let rx_clone = tx.subscribe();
                let ids_clone = Arc::clone(&ids);
                let mut id_ = ids_clone.lock().await;
                let id = *id_;
                *id_ += 1;
                handles.retain(|handle: &JoinHandle<()>| !handle.is_finished());
                handles.push(tokio::spawn(async move {
                    println!("Connection opened: {:?} with id {}", addr.ip(), id);
                    process(socket, tx_clone, rx_clone, id).await;
                }));
            }
        }
    }
}

async fn process(mut socket: TcpStream, tx: Sender<Vec<u8>>, mut rx: Receiver<Vec<u8>>, id: u32) {
    let mut buf = [0; 1024];

    loop {
        tokio::select! {
            signal = socket.read(&mut buf) => {
                let size = match signal {
                    Ok(0) => {
                        println!("Connection closed.");
                        return;
                    },
                    Ok(n) => n,
                    Err(err) => {
                        eprintln!("{:?}", err);
                        return;
                    }
                };

                let mut message = id.to_be_bytes().to_vec();
                message.extend_from_slice(&buf[..size]);

                buf.fill(0);

                if let Err(err) = tx.send(message) {
                    eprint!("{:?}", err);
                    return;
                }
            }

            signal = rx.recv() => {
                let message = match signal {
                    Ok(message) => message,
                    Err(err) => {
                        eprint!("{:?}", err);
                        return;
                    }
                };

                let sender_id: [u8; 4] = message[..4].try_into().unwrap();
                if id != u32::from_be_bytes(sender_id) {
                    let buffer = &message[4..];
                    if let Err(err) = socket.write_all(buffer).await {
                        eprint!("{:?}", err);
                        return;
                    }
                }
            }
        }
    }
}
