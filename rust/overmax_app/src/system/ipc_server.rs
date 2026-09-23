//! Overmax IPC Service & Domain RPC Dispatcher.
//!
//! Exposes real-time game events over SSE (`/events`) and query/control capabilities
//! over JSON-RPC 2.0 (`/rpc`) via the pure `system::transport` loopback engine.

use crate::system::transport::{
    format_sse_frame as transport_format_sse, now_ms, spawn_loopback_service, LoopbackServerConfig,
    TransportHandle,
};
use overmax_core::{GameSessionState, PlayContext, SceneType};
use overmax_data::{RecordManager, VArchiveDB};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::mpsc::{Sender, SyncSender};
use std::sync::{Arc, Mutex};

pub const PROTOCOL_ID: &str = "overmax-ipc/1";

// ─────────────────────────────────────────────────────────────────────────────
// Public Handles & Types
// ─────────────────────────────────────────────────────────────────────────────

/// Handle for the GUI thread to publish real-time game events.
#[derive(Clone)]
pub struct IpcPublisher {
    tx: SyncSender<IpcEvent>,
}

impl IpcPublisher {
    pub fn publish(&self, event: IpcEvent) {
        let _ = self.tx.try_send(event);
    }
}

/// Handle to signal graceful shutdown of the IPC server.
pub type IpcServerHandle = TransportHandle;

/// Currently bound port slot (`None` = inactive/disabled).
pub type BoundPortSlot = Arc<Mutex<Option<u16>>>;

/// Inbound control commands from IPC clients to GUI thread.
#[derive(Clone, Debug)]
pub enum IpcCommand {
    SetOverlayVisibility(bool),
}

/// Shared data access handles for read-only RPC queries.
#[derive(Clone)]
pub struct IpcDataSources {
    pub varchive_db: Arc<VArchiveDB>,
    pub record_manager: Arc<RecordManager>,
}

/// Spawns the IPC manager and SSE hub via the loopback transport engine.
pub fn spawn_ipc_manager(
    root: PathBuf,
    settings: Arc<Mutex<Value>>,
    app_version: &'static str,
    cmd_tx: Sender<IpcCommand>,
    data: IpcDataSources,
) -> (IpcPublisher, IpcServerHandle, BoundPortSlot) {
    let transport_config = LoopbackServerConfig {
        root,
        protocol_id: PROTOCOL_ID,
        manifest_name: "overmax",
        port_band: overmax_data::config::settings::IPC_PORT_BAND,
    };

    let (hub_tx, handle, bound_slot) = spawn_loopback_service(
        transport_config,
        move || read_ipc_settings(&settings),
        move |event: &IpcEvent, seq| format_sse_frame(event, seq, app_version),
        || {
            latest_snapshot().map(|state| IpcEvent::StateSnapshot {
                payload: snapshot_json(&state),
            })
        },
        move |body| dispatch_rpc_endpoint(body, &cmd_tx, &data),
    );

    (IpcPublisher { tx: hub_tx }, handle, bound_slot)
}

// ─────────────────────────────────────────────────────────────────────────────
// Event Types & Envelope Serialization
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Default)]
pub struct SongMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub floor_name: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type")]
pub enum IpcEvent {
    SceneDetected {
        scene: String,
    },
    SongDetected {
        song_id: i32,
        mode: String,
        diff: String,
        rate: f32,
        is_max_combo: bool,
        #[serde(flatten)]
        meta: SongMeta,
    },
    PlayVerified {
        song_id: i32,
        mode: String,
        diff: String,
        rate: f32,
        is_max_combo: bool,
        is_pb: bool,
        #[serde(flatten)]
        meta: SongMeta,
    },
    ContextUpdated {
        context: Option<Value>,
    },
    StateSnapshot {
        payload: Value,
    },
}

impl IpcEvent {
    fn sse_name(&self) -> &'static str {
        match self {
            IpcEvent::SceneDetected { .. } => "scene_detected",
            IpcEvent::SongDetected { .. } => "song_detected",
            IpcEvent::PlayVerified { .. } => "play_verified",
            IpcEvent::ContextUpdated { .. } => "context_updated",
            IpcEvent::StateSnapshot { .. } => "state_snapshot",
        }
    }

    fn payload(&self) -> Value {
        match self {
            IpcEvent::StateSnapshot { payload } => payload.clone(),
            _ => {
                let mut val = serde_json::to_value(self).unwrap_or(Value::Null);
                if let Value::Object(ref mut map) = val {
                    map.remove("type");
                }
                val
            }
        }
    }
}

fn format_sse_frame(event: &IpcEvent, seq: u64, app_version: &str) -> String {
    let name = event.sse_name();
    let data = json!({
        "protocol": PROTOCOL_ID,
        "type": name,
        "seq": seq,
        "ts_ms": now_ms(),
        "app_version": app_version,
        "payload": event.payload(),
    });
    transport_format_sse(name, &data.to_string())
}

fn read_ipc_settings(settings: &Arc<Mutex<Value>>) -> (bool, u16) {
    let default_port = overmax_data::config::settings::IpcSettings::default().port;
    let guard = settings.lock().ok();
    let Some(v) = guard else {
        return (false, default_port);
    };
    let ipc = v.get("ipc");
    let enabled = ipc
        .and_then(|i| i.get("enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let port = ipc
        .and_then(|i| i.get("port"))
        .and_then(Value::as_u64)
        .and_then(|p| u16::try_from(p).ok())
        .filter(|&p| (1024..=65535).contains(&p))
        .unwrap_or(default_port);
    (enabled, port)
}

// ─────────────────────────────────────────────────────────────────────────────
// Domain RPC Dispatcher (JSON-RPC 2.0)
// ─────────────────────────────────────────────────────────────────────────────

fn dispatch_rpc_endpoint(
    body: &str,
    cmd_tx: &Sender<IpcCommand>,
    data: &IpcDataSources,
) -> (u16, &'static str, Value) {
    let outcome = dispatch_rpc(body, cmd_tx, data);
    let (status, reason) = if outcome.is_error {
        (500, "Internal Server Error")
    } else {
        (200, "OK")
    };
    (status, reason, outcome.body)
}

struct RpcOutcome {
    body: Value,
    is_error: bool,
}

fn rpc_error(id: Value, code: i32, message: &str) -> RpcOutcome {
    RpcOutcome {
        body: json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}}),
        is_error: true,
    }
}

fn dispatch_rpc(body: &str, cmd_tx: &Sender<IpcCommand>, data: &IpcDataSources) -> RpcOutcome {
    let Ok(req) = serde_json::from_str::<Value>(body) else {
        return rpc_error(Value::Null, -32700, "Parse error");
    };

    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let Some(method) = req.get("method").and_then(Value::as_str) else {
        return rpc_error(id, -32600, "Invalid Request");
    };

    let params = req.get("params").and_then(|p| p.as_array()).cloned();
    let arg = |i: usize| -> Option<Value> { params.as_ref().and_then(|p| p.get(i).cloned()) };

    match method {
        "list_methods" => RpcOutcome {
            body: json!({
                "jsonrpc":"2.0","id":id,
                "result": {"methods": ["get_current_context", "get_recommendations", "get_song_info",
                                       "get_recent_plays", "set_overlay_visibility", "list_methods"],
                           "protocol": PROTOCOL_ID}
            }),
            is_error: false,
        },
        "get_current_context" => match latest_snapshot() {
            Some(state) => RpcOutcome {
                body: json!({"jsonrpc":"2.0","id":id,"result":snapshot_json(&state)}),
                is_error: false,
            },
            None => rpc_error(id, -32001, "no session snapshot available"),
        },
        "get_recommendations" => match latest_recommendations() {
            Some(recs) => RpcOutcome {
                body: json!({"jsonrpc":"2.0","id":id,"result":recs}),
                is_error: false,
            },
            None => rpc_error(id, -32001, "no recommendations available"),
        },
        "get_song_info" => {
            let Some(song_id) = arg(0).and_then(|v| v.as_i64().map(|n| n as i32)) else {
                return rpc_error(id, -32602, "invalid params: song_id (number) required");
            };
            const MODE_NAMES: [&str; 4] = ["4B", "5B", "6B", "8B"];
            const DIFF_NAMES: [&str; 4] = ["NM", "HD", "MX", "SC"];
            let result = data.varchive_db.search_by_id(song_id).map(|song| {
                let patterns: Vec<Value> = song
                    .patterns
                    .iter()
                    .enumerate()
                    .flat_map(|(mi, per_mode)| {
                        per_mode.iter().enumerate().filter_map(move |(di, p)| {
                            p.as_ref().map(move |pat| {
                                json!({
                                    "mode": MODE_NAMES[mi],
                                    "diff": DIFF_NAMES[di],
                                    "level": pat.level,
                                    "floor_name": pat.floor_name.as_deref(),
                                })
                            })
                        })
                    })
                    .collect();
                json!({
                    "song_id": song_id,
                    "title": song.name,
                    "composer": song.composer,
                    "dlc": song.dlc_code,
                    "patterns": patterns,
                })
            });
            match result {
                Some(info) => RpcOutcome {
                    body: json!({"jsonrpc":"2.0","id":id,"result":info}),
                    is_error: false,
                },
                None => rpc_error(id, -32001, "song not found"),
            }
        }
        "get_recent_plays" => {
            let mode = arg(0)
                .and_then(|v| v.as_str().and_then(overmax_core::Mode::from_str))
                .or_else(|| latest_snapshot().and_then(|s| s.last_stable_context.map(|c| c.mode)));
            let limit = arg(1)
                .and_then(|v| v.as_u64())
                .map(|n| n.clamp(1, 100) as usize)
                .unwrap_or(20);
            let Some(mode) = mode else {
                return rpc_error(
                    id,
                    -32602,
                    "invalid params: mode required (no active session to infer from)",
                );
            };
            let plays = data.record_manager.get_recent_records(mode, limit);
            let entries: Vec<Value> = plays
                .iter()
                .map(|r| {
                    json!({
                        "song_id": r.song_id,
                        "mode": r.button_mode.as_str(),
                        "diff": r.difficulty.as_str(),
                        "rate": r.rate,
                        "is_max_combo": r.is_max_combo,
                        "played_at_unix": r.updated_at,
                    })
                })
                .collect();
            RpcOutcome {
                body: json!({"jsonrpc":"2.0","id":id,"result":{"plays":entries}}),
                is_error: false,
            }
        }
        "set_overlay_visibility" => {
            let visible = arg(0).and_then(|v| v.as_bool()).unwrap_or(false);
            let _ = cmd_tx.send(IpcCommand::SetOverlayVisibility(visible));
            RpcOutcome {
                body: json!({"jsonrpc":"2.0","id":id,"result":null}),
                is_error: false,
            }
        }
        other => rpc_error(id, -32601, &format!("method not found: {other}")),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// State & Recommendation Snapshot Caches
// ─────────────────────────────────────────────────────────────────────────────

/// Scene status is current; retained song context may belong to an earlier scene.
/// Keep this IPC projection separate from the pipeline's verified session state.
#[derive(Clone)]
struct IpcSnapshot {
    scene: SceneType,
    is_stable: bool,
    is_fullscreen: bool,
    last_stable_context: Option<PlayContext>,
}

static LATEST_STATE: Mutex<Option<IpcSnapshot>> = Mutex::new(None);

pub fn update_latest_state(state: GameSessionState) {
    if let Ok(mut slot) = LATEST_STATE.try_lock() {
        let last_stable_context = if state.is_stable {
            state.context
        } else {
            slot.as_mut().and_then(|s| s.last_stable_context.take())
        };
        *slot = Some(IpcSnapshot {
            scene: state.scene,
            is_stable: state.is_stable,
            is_fullscreen: state.is_fullscreen,
            last_stable_context,
        });
    }
}

fn latest_snapshot() -> Option<IpcSnapshot> {
    LATEST_STATE.lock().ok().and_then(|s| s.clone())
}

fn snapshot_json(state: &IpcSnapshot) -> Value {
    let context = state.last_stable_context.as_ref().map(|ctx| {
        json!({
            "song_id": ctx.song_id,
            "mode": ctx.mode.as_str(),
            "diff": ctx.diff.as_str(),
            "rate": ctx.rate,
            "is_max_combo": ctx.is_max_combo,
        })
    });
    json!({
        "scene": format!("{:?}", state.scene),
        "stable": state.is_stable,
        "fullscreen": state.is_fullscreen,
        "context": context,
    })
}

static LATEST_RECOMMENDATIONS: Mutex<Option<Value>> = Mutex::new(None);

pub fn update_latest_recommendations(recs: Value) {
    if let Ok(mut slot) = LATEST_RECOMMENDATIONS.try_lock() {
        *slot = Some(recs);
    }
}

fn latest_recommendations() -> Option<Value> {
    LATEST_RECOMMENDATIONS.lock().ok().and_then(|r| r.clone())
}
