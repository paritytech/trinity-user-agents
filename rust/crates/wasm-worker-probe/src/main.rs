//! Runs a wasm32 Worker product against a TrUAPI host.
//!
//! The host is any product-frame endpoint a `truapi-host` process prints
//! (`TRUAPI_FRAME_URL`): the runner opens the WebSocket, instantiates the
//! module under wasmi with a fuel budget and a memory cap, calls `on_start`,
//! then relays every binary frame to `on_frame` and every frame the guest
//! emits back to the socket. Lifecycle is the runner's: `--suspend-after`
//! calls `on_suspend` and, a second later, `on_resume`.
//!
//! Every frame crossing in either direction and every line the guest logs is
//! written as one JSON object per line, to stdout and to `--transcript`.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::Parser;
use futures::{SinkExt, StreamExt};
use tokio::io::AsyncWriteExt;
use tokio_tungstenite::tungstenite::Message;

mod sandbox;
mod transcript;

use sandbox::Sandbox;
use transcript::{Direction, Transcript};

/// Command line.
#[derive(Debug, Parser)]
#[command(name = "wasm-worker-probe")]
struct Args {
    /// The wasm32 module to run as the product.
    #[arg(long, env = "TRUAPI_WASM_MODULE")]
    module: PathBuf,
    /// Product-frame endpoint of the host, `ws://127.0.0.1:<port>` or
    /// `ws+unix:<path>`.
    #[arg(long, env = "TRUAPI_FRAME_URL")]
    frame_url: String,
    /// Append the transcript here as well as to stdout.
    #[arg(long)]
    transcript: Option<PathBuf>,
    /// Fuel the guest may burn per entry point; exhaustion is a trap.
    #[arg(long, default_value_t = 50_000_000)]
    fuel: u64,
    /// Largest guest linear memory, in bytes.
    #[arg(long, default_value_t = 16 * 1024 * 1024)]
    memory_bytes: usize,
    /// Call `on_suspend` this many seconds in, then `on_resume` a second later.
    #[arg(long)]
    suspend_after: Option<u64>,
    /// Exit after this many seconds; otherwise run until the host closes.
    #[arg(long)]
    run_for: Option<u64>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let mut transcript = Transcript::open(args.transcript.as_deref()).await?;
    let module = tokio::fs::read(&args.module)
        .await
        .with_context(|| format!("read {}", args.module.display()))?;
    let mut sandbox = Sandbox::new(&module, args.fuel, args.memory_bytes)?;
    transcript
        .note(serde_json::json!({
            "kind": "sandbox",
            "module": args.module.display().to_string(),
            "moduleBytes": module.len(),
            "fuelPerTurn": args.fuel,
            "memoryBytes": args.memory_bytes,
        }))
        .await?;

    let (mut socket_tx, mut socket_rx) = connect(&args.frame_url).await?.split();
    transcript
        .note(serde_json::json!({ "kind": "connected", "frameUrl": args.frame_url }))
        .await?;

    turn(
        &mut sandbox,
        &mut socket_tx,
        &mut transcript,
        sandbox::Turn::Start,
    )
    .await?;

    let mut deadline = args
        .run_for
        .map(|seconds| Box::pin(tokio::time::sleep(Duration::from_secs(seconds))));
    let mut suspend = args
        .suspend_after
        .map(|seconds| Box::pin(tokio::time::sleep(Duration::from_secs(seconds))));
    let mut resume: Option<std::pin::Pin<Box<tokio::time::Sleep>>> = None;

    loop {
        tokio::select! {
            message = socket_rx.next() => {
                let Some(message) = message else {
                    transcript.note(serde_json::json!({ "kind": "closed", "by": "host" })).await?;
                    break;
                };
                let frame = match message? {
                    Message::Binary(bytes) => bytes,
                    Message::Close(_) => {
                        transcript.note(serde_json::json!({ "kind": "closed", "by": "host" })).await?;
                        break;
                    }
                    _ => continue,
                };
                transcript.frame(Direction::HostToGuest, &frame).await?;
                turn(&mut sandbox, &mut socket_tx, &mut transcript, sandbox::Turn::Frame(frame)).await?;
            }
            _ = async { suspend.as_mut().expect("checked").await }, if suspend.is_some() => {
                suspend = None;
                transcript.note(serde_json::json!({ "kind": "lifecycle", "call": "on_suspend" })).await?;
                turn(&mut sandbox, &mut socket_tx, &mut transcript, sandbox::Turn::Suspend).await?;
                resume = Some(Box::pin(tokio::time::sleep(Duration::from_secs(1))));
            }
            _ = async { resume.as_mut().expect("checked").await }, if resume.is_some() => {
                resume = None;
                transcript.note(serde_json::json!({ "kind": "lifecycle", "call": "on_resume" })).await?;
                turn(&mut sandbox, &mut socket_tx, &mut transcript, sandbox::Turn::Resume).await?;
            }
            _ = async { deadline.as_mut().expect("checked").await }, if deadline.is_some() => {
                transcript.note(serde_json::json!({ "kind": "closed", "by": "runner" })).await?;
                let _ = socket_tx.send(Message::Close(None)).await;
                break;
            }
        }
    }
    transcript
        .note(serde_json::json!({
            "kind": "summary",
            "turns": sandbox.turns(),
            "fuelBurnedTotal": sandbox.fuel_burned(),
            "memoryBytesNow": sandbox.memory_bytes(),
        }))
        .await?;
    Ok(())
}

/// Run one guest turn, then log what the guest wrote and send what it emitted.
async fn turn<S>(
    sandbox: &mut Sandbox,
    socket: &mut S,
    transcript: &mut Transcript,
    turn: sandbox::Turn,
) -> Result<()>
where
    S: SinkExt<Message> + Unpin,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    let outcome = sandbox.run(turn);
    for line in sandbox.take_logs() {
        transcript.guest(&line).await?;
    }
    let outcome = outcome?;
    transcript
        .note(serde_json::json!({ "kind": "turn", "fuelBurned": outcome.fuel_burned }))
        .await?;
    for bytes in outcome.frames {
        transcript.frame(Direction::GuestToHost, &bytes).await?;
        socket.send(Message::Binary(bytes)).await?;
    }
    Ok(())
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<Box<dyn Stream>>>;

/// What the socket is made of, so TCP and Unix connections share one type.
trait Stream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send {}
impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send> Stream for T {}

async fn connect(frame_url: &str) -> Result<Socket> {
    let stream: Box<dyn Stream> = if let Some(path) = frame_url.strip_prefix("ws+unix:") {
        Box::new(
            tokio::net::UnixStream::connect(path)
                .await
                .with_context(|| format!("connect {path}"))?,
        )
    } else if let Some(address) = frame_url.strip_prefix("ws://") {
        Box::new(
            tokio::net::TcpStream::connect(address)
                .await
                .with_context(|| format!("connect {address}"))?,
        )
    } else {
        bail!("frame url must start with ws:// or ws+unix:, got {frame_url}");
    };
    let mut stream = stream;
    stream.flush().await?;
    let request = tokio_tungstenite::tungstenite::http::Request::builder()
        .uri("ws://localhost/")
        .header("Host", "localhost")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header(
            "Sec-WebSocket-Key",
            tokio_tungstenite::tungstenite::handshake::client::generate_key(),
        )
        .body(())?;
    let (socket, _) =
        tokio_tungstenite::client_async(request, tokio_tungstenite::MaybeTlsStream::Plain(stream))
            .await
            .context("websocket handshake")?;
    Ok(socket)
}
