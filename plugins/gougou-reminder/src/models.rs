use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderSchedule {
    pub enabled: bool,
    pub hour: u8,
    pub minute: u8,
    pub precise: bool,
    pub quiet_weekdays: Vec<u8>,
    pub paused_until: Option<String>,
    pub skip_dates: Vec<String>,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderStatus {
    pub supported: bool,
    pub permission: String,
    pub exact_alarm_allowed: bool,
    pub effective_precise: bool,
    pub scheduled_count: u32,
    #[serde(default)]
    pub background_settings_available: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationTarget {
    pub target_date: Option<String>,
}
