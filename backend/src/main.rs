mod username;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashSet, VecDeque},
    sync::{Arc, Mutex},
};
use tokio::sync::broadcast;
use tracing::warn;

use self::username::Username;

const MESSAGE_QUEUE_SIZE: usize = 20;
const CHANNEL_CAPACITY: usize = 100;

/// Main application state.
struct AppState {
    /// This tracks which usernames have been taken, since all usernames must be unique.
    user_set: Mutex<HashSet<Username>>,
    /// Channel used to send messages to all connected clients.
    tx: broadcast::Sender<AppMsg>,
    /// A queue to maintain previous chat messages.
    msg_queue: Mutex<VecDeque<ChatMsg>>,
}

impl AppState {
    fn new(tx: broadcast::Sender<AppMsg>) -> Self {
        Self {
            user_set: Mutex::new(HashSet::default()),
            tx,
            msg_queue: Mutex::new(VecDeque::with_capacity(MESSAGE_QUEUE_SIZE)),
        }
    }

    /// Send a message in the chat, storing it in the message queue too.
    fn send_chat(&self, msg: ChatMsg) {
        let mut msg_queue = self.msg_queue.lock().unwrap();
        if msg_queue.len() == MESSAGE_QUEUE_SIZE {
            msg_queue.pop_front();
        }
        msg_queue.push_back(msg.clone());
        let _ = self.tx.send(AppMsg::ChatMsg(msg));
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
enum AppMsg {
    Error(String),
    ChatMsg(ChatMsg),
    ChatMsgList { msgs: Vec<ChatMsg> },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ChatMsg {
    user: Username,
    contents: String,
}

#[shuttle_runtime::main]
async fn main() -> shuttle_axum::ShuttleAxum {
    let (tx, _rx) = broadcast::channel(CHANNEL_CAPACITY);
    let app_state = Arc::new(AppState::new(tx));
    let router = Router::new()
        .route("/websocket", get(websocket_handler))
        .with_state(app_state);

    Ok(router.into())
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| websocket(socket, state))
}

/// Handles a single websocket connection.
async fn websocket(stream: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = stream.split();

    let mut username = Username::default();
    while let Some(Ok(message)) = receiver.next().await {
        let Message::Text(name) = message else {
            warn!("got strange first message: {message:?}");
            continue;
        };
        let name = Username::from(name);
        // Require unique names
        if state.user_set.lock().unwrap().contains(&name) {
            let _ = sender
                .send(Message::Text(
                    serde_json::to_string(&AppMsg::Error("Username already taken".to_owned()))
                        .expect("message serialization shouldn't fail"),
                ))
                .await;
            return;
        }
        state.user_set.lock().unwrap().insert(name.clone());
        username = name;
        break;
    }

    let mut rx = state.tx.subscribe();

    // Send all previous messages in the queue
    let _ = state.tx.send(AppMsg::ChatMsgList {
        msgs: state
            .msg_queue
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .collect::<Vec<ChatMsg>>(),
    });

    // Receives messages from the global receiver and forwards them to the conected client.
    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender
                .send(Message::Text(
                    serde_json::to_string(&msg).expect("message serialization shouldn't fail"),
                ))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    let state_clone = state.clone();
    let name = username.clone();

    // Receives messages from the websocket and then forwards them to the global channel as a
    // ChatMsg.
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = receiver.next().await {
            let msg = ChatMsg {
                user: name.clone(),
                contents: text,
            };
            state_clone.send_chat(msg);
        }
    });

    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    };

    state.user_set.lock().unwrap().remove(&username);
}
