//! WebSocket lifecycle: connect to the telemetry feed, subscribe to a chain
//! by genesis hash, reconnect with backoff on drop, and drive a 1s ticker so
//! a block stall can be detected even when the feed goes silent.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

use crate::feed::parse_feed_message;
use crate::health::NotifyEngine;
use crate::state::TelemetryState;
use crate::{ConnectionStatus, TelemetryDelegate};

/// rustls 0.23 requires a process-wide CryptoProvider to be installed before
/// any TLS connection; nothing does this implicitly for tokio-tungstenite's
/// rustls-tls-webpki-roots feature, so without this every connect attempt
/// panics the telemetry thread instead of erroring.
static INSTALL_CRYPTO_PROVIDER: std::sync::Once = std::sync::Once::new();

fn ensure_crypto_provider() {
    INSTALL_CRYPTO_PROVIDER.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// Spawns the telemetry thread, detached. It owns everything it needs and runs
/// until `stop` is set, then returns on its own.
///
/// Nothing joins it, which is why neither stopping nor dropping a client can
/// block: a join would have to wait for the thread to notice the flag, which
/// takes until its next tick — or, if it is between reconnect attempts, until
/// its backoff sleep ends. Callers must therefore expect a few late delegate
/// callbacks after stopping.
pub fn spawn(
    feed_url: String,
    genesis: String,
    block_stall_secs: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    delegate: Arc<dyn TelemetryDelegate>,
) {
    ensure_crypto_provider();
    std::thread::Builder::new()
        .name("midnight-telemetry".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to start telemetry runtime");
            rt.block_on(run_loop(feed_url, genesis, block_stall_secs, delegate, stop));
        })
        .expect("failed to spawn telemetry thread");
}

async fn run_loop(
    feed_url: String,
    genesis: String,
    block_stall_secs: Arc<AtomicU64>,
    delegate: Arc<dyn TelemetryDelegate>,
    stop: Arc<AtomicBool>,
) {
    let mut retry: u32 = 0;
    while !stop.load(Ordering::SeqCst) {
        delegate.on_status_changed(ConnectionStatus::Connecting);
        let connected_ok =
            connect_and_run(&feed_url, &genesis, &block_stall_secs, &delegate, &stop).await;
        if stop.load(Ordering::SeqCst) {
            break;
        }
        if connected_ok {
            retry = 0;
        }
        delegate.on_status_changed(ConnectionStatus::Reconnecting);
        let backoff_ms = (2_000f64 * 1.5f64.powi(retry as i32)).min(30_000.0) as u64;
        retry += 1;
        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
    }
}

/// Returns true if the connection was established at all (used only to reset
/// the backoff counter — a connection that drops after a while shouldn't make
/// the next attempt wait as long as a connection that never came up).
async fn connect_and_run(
    feed_url: &str,
    genesis: &str,
    block_stall_secs: &AtomicU64,
    delegate: &Arc<dyn TelemetryDelegate>,
    stop: &Arc<AtomicBool>,
) -> bool {
    let (ws_stream, _) = match tokio_tungstenite::connect_async(feed_url).await {
        Ok(pair) => pair,
        Err(_) => return false,
    };
    let (mut write, mut read) = ws_stream.split();
    if write.send(Message::text(format!("subscribe:{genesis}"))).await.is_err() {
        return false;
    }
    delegate.on_status_changed(ConnectionStatus::Live);

    let mut state = TelemetryState::new(Instant::now());
    let mut engine = NotifyEngine::new();
    let mut ticker = tokio::time::interval(Duration::from_secs(1));

    loop {
        if stop.load(Ordering::SeqCst) {
            return true;
        }

        tokio::select! {
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        apply_and_emit(&mut state, &mut engine, delegate, block_stall_secs, text.as_str());
                    }
                    Some(Ok(Message::Binary(bin))) => {
                        if let Ok(text) = std::str::from_utf8(&bin) {
                            apply_and_emit(&mut state, &mut engine, delegate, block_stall_secs, text);
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => return true,
                    Some(Err(_)) => return true,
                    _ => {}
                }
            }
            _ = ticker.tick() => {
                let now = Instant::now();
                delegate.on_snapshot(state.snapshot(now));
                emit_alerts(&mut state, &mut engine, delegate, block_stall_secs, now);
            }
        }
    }
}

fn apply_and_emit(
    state: &mut TelemetryState,
    engine: &mut NotifyEngine,
    delegate: &Arc<dyn TelemetryDelegate>,
    block_stall_secs: &AtomicU64,
    raw: &str,
) {
    let now = Instant::now();
    for event in parse_feed_message(raw) {
        state.apply(event, now);
    }
    delegate.on_snapshot(state.snapshot(now));
    emit_alerts(state, engine, delegate, block_stall_secs, now);
}

fn emit_alerts(
    state: &mut TelemetryState,
    engine: &mut NotifyEngine,
    delegate: &Arc<dyn TelemetryDelegate>,
    block_stall_secs: &AtomicU64,
    now: Instant,
) {
    let seconds_since_last_block = state.seconds_since_last_block(now);
    let peer_drops = state.peer_drop_candidates();
    // Re-read every evaluation so a threshold change applies immediately,
    // without tearing down the connection.
    let threshold = f64::from_bits(block_stall_secs.load(Ordering::Relaxed));
    for alert in engine.evaluate(seconds_since_last_block, &peer_drops, threshold, now) {
        delegate.on_alert(alert);
    }
}
