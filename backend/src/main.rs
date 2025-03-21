use chrono::Utc;
use rocket::futures::{stream::SplitSink, SinkExt, StreamExt};
use rocket::tokio::sync::Mutex;
use rocket::State;
use rocket_ws::{stream::DuplexStream, Channel, Message, WebSocket};
use serde_json::json;
use std::{
    collections::HashMap,
    sync::atomic::{AtomicUsize, Ordering},
};

static USER_ID_COUNTER: AtomicUsize = AtomicUsize::new(1);

#[derive(Default)]
struct ChatRoom {
    connections: Mutex<HashMap<usize, SplitSink<DuplexStream, Message>>>,
}

impl ChatRoom {
    pub async fn add(&self, id: usize, sink: SplitSink<DuplexStream, Message>) {
        let mut conns = self.connections.lock().await;
        conns.insert(id, sink);
    }

    pub async fn broadcast_message(&self, message: Message, author_id: usize) {
        let chat_message = common::ChatMessage {
            message: message.to_string(),
            author: format!("User #{}", author_id),
            created_at: Utc::now().naive_utc(),
        };
        let mut conns = self.connections.lock().await;
        for (_id, sink) in conns.iter_mut() {
            let _ = sink
                .send(Message::Text(json!(chat_message).to_string()))
                .await;
        }
    }

    pub async fn remove(&self, id: usize) {
        let mut conns = self.connections.lock().await;
        conns.remove(&id);
    }
}

#[rocket::get("/")]
fn chat<'r>(ws: WebSocket, state: &'r State<ChatRoom>) -> Channel<'r> {
    ws.channel(move |stream| {
        Box::pin(async move {
            let user_id = USER_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
            let (ws_sink, mut ws_stream) = stream.split();
            state.add(user_id, ws_sink).await;

            while let Some(message) = ws_stream.next().await {
                state.broadcast_message(message?, user_id).await;
            }

            state.remove(user_id).await;

            Ok(())
        })
    })
}

#[rocket::main]
async fn main() {
    let _ = rocket::build()
        .mount("/", rocket::routes![chat])
        .manage(ChatRoom::default())
        .launch()
        .await;
}
