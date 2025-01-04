mod username;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{
        header::{AUTHORIZATION, CONTENT_TYPE},
        HeaderValue, Method,
    },
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures::{sink::SinkExt, stream::StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashSet, VecDeque},
    sync::{Arc, Mutex},
};
use tokio::sync::broadcast;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::{info, warn};

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
    secrets: shuttle_runtime::SecretStore,
}

impl AppState {
    fn new(tx: broadcast::Sender<AppMsg>, secrets: shuttle_runtime::SecretStore) -> Self {
        Self {
            user_set: Mutex::new(HashSet::default()),
            tx,
            msg_queue: Mutex::new(VecDeque::with_capacity(MESSAGE_QUEUE_SIZE)),
            secrets,
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
async fn main(
    #[shuttle_runtime::Secrets] secrets: shuttle_runtime::SecretStore,
) -> shuttle_axum::ShuttleAxum {
    let (tx, _rx) = broadcast::channel(CHANNEL_CAPACITY);
    let app_state = Arc::new(AppState::new(tx, secrets));

    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST])
        .allow_origin(AllowOrigin::exact(HeaderValue::from_static(
            "http://localhost:8080",
        )))
        .allow_headers([AUTHORIZATION, CONTENT_TYPE]);
    let router = Router::new()
        .route("/websocket", get(websocket_handler))
        .route("/translate", post(translate_handler))
        .layer(cors)
        .with_state(app_state);

    Ok(router.into())
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| websocket(socket, state))
}

#[derive(Debug, Deserialize)]
struct TranslateMessage {
    en_text: String,
}

#[derive(Serialize)]
struct TranslateResponse {
    zh_text: String,
}

async fn translate_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<TranslateMessage>,
) -> Json<TranslateResponse> {
    info!("received request to translate");
    #[derive(Serialize)]
    struct TranslateBody {
        text: Vec<String>,
        target_lang: String,
        source_lang: String,
    }

    #[derive(Deserialize)]
    struct DeepLResponseText {
        text: String,
    }

    #[derive(Deserialize)]
    struct DeepLResponse {
        translations: Vec<DeepLResponseText>,
    }

    let client = Client::new();
    let body = TranslateBody {
        text: vec![payload.en_text],
        target_lang: "EN".to_owned(),
        source_lang: "ZH".to_owned(),
    };
    let mut res = client
        .post("https://api-free.deepl.com/v2/translate")
        .header(
            "Authorization",
            format!(
                "DeepL-Auth-Key {}",
                state.secrets.get("DEEPL_API_KEY").unwrap()
            ),
        )
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&body).unwrap())
        .send()
        .await
        .unwrap()
        .json::<DeepLResponse>()
        .await
        .unwrap();

    let translated = std::mem::take(&mut res.translations[0].text);
    info!("got: {}", &translated);
    Json(TranslateResponse {
        zh_text: translated,
    })
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
