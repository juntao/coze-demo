use serde_json::Value;
use serde_json::Map;
use tg_flows::{listen_to_update, Telegram, Update, UpdateKind, update_handler};
use flowsnet_platform_sdk::logger;

#[no_mangle]
#[tokio::main(flavor = "current_thread")]
pub async fn on_deploy() {
    let telegram_token = std::env::var("telegram_token").unwrap();
    listen_to_update(telegram_token).await;
}

#[update_handler]
async fn handler(update: Update) {
    logger::init();

    let telegram_token = std::env::var("telegram_token").unwrap();
    let placeholder_text = std::env::var("placeholder").unwrap_or("Typing ...".to_string());
    let coze_access_token = std::env::var("coze_access_token").unwrap_or("".to_string());
    let coze_bot_id = std::env::var("coze_bot_id").unwrap_or("".to_string());

    let tele = Telegram::new(telegram_token.to_string());

    if let UpdateKind::Message(msg) = update.kind {
        let chat_id = msg.chat.id;
        log::info!("Received message from {}", chat_id);
        let text = msg.text().unwrap_or("");

        let placeholder = tele
            .send_message(chat_id, &placeholder_text)
            .expect("Error occurs when sending Message to Telegram");

        let mut coze_msg = Map::new();
        coze_msg.insert("role".to_string(), Value::String("user".to_string()));
        coze_msg.insert("content".to_string(), Value::String(text.to_string()));
        coze_msg.insert("content_type".to_string(), Value::String("text".to_string()));

        let mut coze_data = Map::new();
        coze_data.insert("bot_id".to_string(), Value::String(coze_bot_id.to_string()));
        coze_data.insert("user_id".to_string(), Value::String(chat_id.to_string()));
        coze_data.insert("stream".to_string(), Value::Bool(false));
        coze_data.insert("auto_save_history".to_string(), Value::Bool(true));
        coze_data.insert("additional_messages".to_string(), Value::Array(vec![Value::Object(coze_msg)]));

        let client = reqwest::Client::new();
        let res = client
            .post("https://api.coze.cn/v3/chat")
            .header("Authorization", &format!("Bearer {}", coze_access_token))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&coze_data).unwrap())
            .send()
            .await.unwrap();
        let body = res.text().await.unwrap();
        log::info!("Coze resp: {}", &body);

        let bot_status: Value = serde_json::from_str(&body).expect("Error deserializing JSON");
        let coze_chat_id = bot_status.get("data").unwrap().get("id").unwrap().as_str().unwrap().to_string();
        let coze_conversation_id = bot_status.get("data").unwrap().get("conversation_id").unwrap().as_str().unwrap().to_string();

        while true {
            let url = format!("https://api.coze.cn/v3/chat/retrieve?chat_id={}&conversation_id={}", coze_chat_id, coze_conversation_id);
            let res = client
                .get(&url)
                .header("Authorization", &format!("Bearer {}", coze_access_token))
                .header("Content-Type", "application/json")
                .send()
                .await.unwrap();
            let body = res.text().await.unwrap();
            log::info!("Coze resp: {}", &body);

            let bot_status: Value = serde_json::from_str(&body).expect("Error deserializing JSON");
            if bot_status.get("data").unwrap().get("status").unwrap().as_str().unwrap().eq_ignore_ascii_case("completed") {
                break;
            }
        }

        let url = format!("https://api.coze.cn/v3/chat/message/list?chat_id={}&conversation_id={}", coze_chat_id, coze_conversation_id);
        let res = client
            .get(&url)
            .header("Authorization", &format!("Bearer {}", coze_access_token))
            .header("Content-Type", "application/json")
            .send()
            .await.unwrap();
        let body = res.text().await.unwrap();
        log::info!("Coze resp: {}", &body);

        let bot_resp: Value = serde_json::from_str(&body).expect("Error deserializing JSON");
        let bot_msgs = bot_resp.get("data").unwrap().as_array().unwrap();
        let mut has_answered = false;
        let mut has_followed_up = false;
        for bot_msg in bot_msgs {
            log::info!("Bot message {:#?}", bot_msg);
            if bot_msg.get("type").unwrap().as_str().unwrap().eq_ignore_ascii_case("answer") && bot_msg.get("content_type").unwrap().as_str().unwrap().eq_ignore_ascii_case("text") {
                if !has_answered {
                    _ = tele.edit_message_text(chat_id, placeholder.id, bot_msg.get("content").unwrap().as_str().unwrap());
                    has_answered = true;
                }
            }
            if bot_msg.get("type").unwrap().as_str().unwrap().eq_ignore_ascii_case("follow_up") && bot_msg.get("content_type").unwrap().as_str().unwrap().eq_ignore_ascii_case("text") {
                if !has_followed_up {
                    _ = tele.send_message(chat_id, bot_msg.get("content").unwrap().as_str().unwrap());
                    has_followed_up = true;
                }
            }
        }

    }
}
