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

const MESSAGE_QUEUE_SIZE: usize = 20;

/// Main application state.
struct AppState {
    /// This tracks which usernames have been taken, since all usernames must be unique.
    user_set: Mutex<HashSet<String>>,
    /// Channel used to send messages to all connected clients.
    tx: broadcast::Sender<AppMsg>,
    /// A queue to maintain previous chat messages.
    msg_queue: Mutex<VecDeque<ChatMsg>>,
}

impl AppState {
    fn broadcast_msg(&self, msg: ChatMsg) {
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
    user: String,
    contents: String,
}

#[shuttle_runtime::main]
async fn main() -> shuttle_axum::ShuttleAxum {
    let user_set = Mutex::new(HashSet::new());
    let (tx, _rx) = broadcast::channel(100);

    let app_state = Arc::new(AppState {
        user_set,
        tx,
        msg_queue: VecDeque::with_capacity(MESSAGE_QUEUE_SIZE).into(),
    });

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

// This function deals with a single websocket connection, i.e., a single
// connected client / user, for which we will spawn two independent tasks (for
// receiving / sending chat messages).
async fn websocket(stream: WebSocket, state: Arc<AppState>) {
    // By splitting, we can send and receive at the same time.
    let (mut sender, mut receiver) = stream.split();

    // Username gets set in the receive loop, if it's valid.
    let mut username = String::new();
    // Loop until a text message is found.
    while let Some(Ok(message)) = receiver.next().await {
        if let Message::Text(name) = message {
            // If username that is sent by client is not taken, fill username string.
            check_username(&state, &mut username, &name);

            // If not empty we want to quit the loop else we want to quit function.
            if !username.is_empty() {
                break;
            } else {
                // Only send our client that username is taken.
                let _ = sender
                    .send(Message::Text(
                        serde_json::to_string(&AppMsg::Error("Username already taken".to_owned()))
                            .expect("message serialization shouldn't fail"),
                    ))
                    .await;

                return;
            }
        }
    }

    // We subscribe *before* sending the "joined" message, so that we will also
    // display it to our client.
    let mut rx = state.tx.subscribe();

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
            state_clone.broadcast_msg(msg);
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

fn check_username(state: &AppState, string: &mut String, name: &str) {
    let mut user_set = state.user_set.lock().unwrap();

    if !user_set.contains(name) {
        user_set.insert(name.to_owned());

        string.push_str(name);
    }
}
