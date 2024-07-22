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

    // Spawn the first task that will receive broadcast messages and send text
    // messages over the websocket to our client.
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

    // Clone things we want to pass (move) to the receiving task.
    let state_clone = state.clone();
    let name = username.clone();

    // Spawn a task that takes messages from the websocket, prepends the user
    // name, and sends them to all broadcast subscribers.
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = receiver.next().await {
            let msg = ChatMsg {
                user: name.clone(),
                contents: text,
            };
            state_clone.send_chat(msg);
        }
    });

    // If any one of the tasks run to completion, we abort the other.
    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    };

    // Remove username from map so new clients can take it again.
    state.user_set.lock().unwrap().remove(&username);
}
