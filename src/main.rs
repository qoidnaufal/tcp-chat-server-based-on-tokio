use std::net::SocketAddr;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf},
    net::{TcpListener, TcpStream},
    sync::broadcast,
};

// ----- program starts below

type Result<T> = std::result::Result<T, String>;

const ADDR: &str = "0.0.0.0:6969";

#[derive(Clone)]
enum Messages {
    ClientConnected(SocketAddr),
    ClientDisconnected(SocketAddr),
    NewMessage((String, SocketAddr)),
}

// ----- server

async fn server(
    mut writer: WriteHalf<TcpStream>,
    mut rx: broadcast::Receiver<Messages>,
    addr: SocketAddr,
) -> Result<()> {
    loop {
        match rx.recv().await {
            Ok(msg) => match msg {
                Messages::NewMessage((text, other_addr)) => {
                    println!("INFO: client {} send message: {}", other_addr, text);

                    if addr != other_addr {
                        let mut txt = format!(">> {}: ", other_addr);
                        txt.push_str(&text);
                        writer.write(txt.as_bytes()).await.map_err(|err| {
                            format!("Unable to write the message back to the client: {}", err)
                        })?;
                    }
                }
                Messages::ClientDisconnected(other_addr) => {
                    println!("INFO: client {} is disconnected", other_addr);
                }
                Messages::ClientConnected(other_addr) => {
                    println!("INFO: client {} is connected", other_addr);
                }
            },
            Err(err) => {
                eprintln!(
                    "ERROR: unable to receive message sent from the client: {}",
                    err
                );
                return Ok(());
            }
        }
    }
}

// ----- client

// there's an unknown behavior (duplicating message):
// 1. when a new client is connected, it's either:
// a. the tx is being shared so they send the same message, or
// b. the rx is duplicating itself?

// 2. the same when i disconnect the client,
// but interestingly, the duplication gradually ended after 3 messages

async fn client(
    reader: ReadHalf<TcpStream>,
    tx: broadcast::Sender<Messages>,
    addr: SocketAddr,
) -> Result<()> {
    tx.send(Messages::ClientConnected(addr)).map_err(|err| {
        format!(
            "Unable to notify the server that a client is connected: {}",
            err
        )
    })?;

    let mut buff_reader = BufReader::new(reader);
    let mut text = String::new();

    loop {
        match buff_reader.read_line(&mut text).await {
            Ok(0) => {
                tx.send(Messages::ClientDisconnected(addr)).map_err(|err| {
                    format!(
                        "Unable to notify the server that a client is disconnected: {}",
                        err
                    )
                })?;
                break Ok(());
            }
            Ok(_) => {
                tx.send(Messages::NewMessage((text.clone(), addr)))
                    .map_err(|err| format!("Unable to send new message: {}", err))?;
                text.clear();
            }
            Err(err) => eprintln!(
                "ERROR: unable to read bytes from the stream on client {}: {}",
                addr, err
            ),
        }
    }
}

// ----- main

#[tokio::main]
async fn main() -> Result<()> {
    let listener = TcpListener::bind(ADDR)
        .await
        .map_err(|err| format!("Unable to bind {}: {}", ADDR, err))?;

    let (tx, _) = broadcast::channel(10);

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                let tx = tx.clone();
                let rx = tx.subscribe();
                let (reader, writer) = tokio::io::split(stream);

                tokio::spawn(async move { server(writer, rx, addr).await });
                tokio::spawn(async move { client(reader, tx, addr).await });
            }
            Err(e) => {
                format!("Unable to accept new connection: {}", e);
            }
        }
    }
}
