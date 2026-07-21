const COMMANDS: &[&str] = &[
    "get_status",
    "request_permission",
    "sync_schedule",
    "cancel_all",
    "take_notification_target",
    "open_background_settings",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .ios_path("ios")
        .build();
}
