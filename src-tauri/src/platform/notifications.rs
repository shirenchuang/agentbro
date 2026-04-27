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
    let truncated = if question.len() > 80 {
        format!("{}...", &question[..77])
    } else {
        question.to_string()
    };
    let _ = app
        .notification()
        .builder()
        .title("Question")
        .body(truncated)
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
    let truncated = if summary.len() > 80 {
        format!("{}...", &summary[..77])
    } else {
        summary.to_string()
    };
    let _ = app
        .notification()
        .builder()
        .title("Task Complete")
        .body(truncated)
        .show();
}
