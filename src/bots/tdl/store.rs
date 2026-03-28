#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ForwardedMessage {
    pub(super) source_link: String,
    pub(super) link_id: String,
    pub(super) target_chat_id: i64,
    pub(super) target_thread_id: Option<i32>,
    pub(super) message_ids: Vec<i32>,
    pub(super) forwarded_at: String,
}
