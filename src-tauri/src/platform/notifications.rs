use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

/// Send a native macOS notification for a blocking event that was suppressed
/// (user is looking at the terminal, so we degrade to notification instead of overlay).
pub fn send_permission_notification(app: &AppHandle, tool_name: &str) {
    let _ = app
        .notification()
        .builder()
        .title("Permission Request")
        .body(format!("{} needs approval", tool_name))
        .show();
}

pub fn send_question_notification(app: &AppHandle, question: &str) {
    let _ = app
        .notification()
        .builder()
        .title("Question")
        .body(notification_body(question))
        .show();
}

pub fn send_plan_notification(app: &AppHandle, plan_title: &str) {
    let _ = app
        .notification()
        .builder()
        .title("Plan Approval")
        .body(plan_title.to_string())
        .show();
}

pub fn send_completion_notification(app: &AppHandle, summary: &str) {
    let _ = app
        .notification()
        .builder()
        .title("Task Complete")
        .body(notification_body(summary))
        .show();
}

fn notification_body(text: &str) -> String {
    if text.chars().take(81).count() <= 80 {
        return text.to_string();
    }

    let mut shortened = text.chars().take(77).collect::<String>();
    shortened.push_str("...");
    shortened
}

#[cfg(test)]
mod tests {
    use super::notification_body;

    #[test]
    fn preserves_unicode_notification_at_character_limit() {
        let text = "中🙂".repeat(40);
        assert_eq!(notification_body(&text), text);
    }

    #[test]
    fn shortens_long_unicode_notification_at_character_boundary() {
        let prefix = "中🙂".repeat(38);
        let text = format!("{prefix}文🙂继续处理");
        assert_eq!(notification_body(&text), format!("{prefix}文..."));
    }
}
