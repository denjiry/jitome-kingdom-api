use crate::domain::model::UserId;
use crate::wrapper::unixtime::UnixTime;
use serde::*;

#[derive(Serialize, Deserialize, Clone)]
pub struct User {
    pub id: UserId,
    pub screen_name: Option<String>,
    pub display_name: String,
    pub point: u64, // みょんポイント
    pub created_at: UnixTime,
    pub subject: String,
}

impl User {
    pub fn new(subject: String, screen_name: Option<String>, display_name: String) -> Self {
        User {
            id: UserId::new(),
            screen_name,
            display_name,
            point: 0,
            created_at: UnixTime::now(),
            subject,
        }
    }
}
