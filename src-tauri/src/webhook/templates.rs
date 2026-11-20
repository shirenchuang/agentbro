// Webhook message templates for DingTalk and Feishu

/// Event type for notification
#[derive(Debug, Clone, PartialEq)]
pub enum NotificationEvent {
    SessionStart,
    SessionStop,
    ToolUse {
        tool_name: String,
    },
    Completion {
        summary: String,
    },
    Error {
        message: String,
    },
    WaitingApproval {
        tool_name: String,
        detail: Option<String>,
    },
    WaitingInput {
        question: String,
    },
    PlanApproval {
        title: String,
    },
    Custom {
        title: String,
        body: String,
    },
}

pub fn event_key(event: &NotificationEvent) -> &'static str {
    match event {
        NotificationEvent::SessionStart => "session_start",
        NotificationEvent::SessionStop => "session_stop",
        NotificationEvent::ToolUse { .. } => "tool_use",
        NotificationEvent::Completion { .. } => "task_complete",
        NotificationEvent::Error { .. } => "error",
        NotificationEvent::WaitingApproval { .. } => "waiting_approval",
        NotificationEvent::WaitingInput { .. } => "waiting_input",
        NotificationEvent::PlanApproval { .. } => "plan_approval",
        NotificationEvent::Custom { .. } => "custom",
    }
}

/// Build DingTalk markdown message body
pub fn dingtalk_markdown(
    event: &NotificationEvent,
    source: &str,
    session_id: &str,
    language: &str,
) -> serde_json::Value {
    let (title, text) = event_to_text(event, source, session_id, language);
    serde_json::json!({
        "msgtype": "markdown",
        "markdown": {
            "title": title,
            "text": text
        }
    })
}

/// Build Feishu interactive card message body
pub fn feishu_interactive(
    event: &NotificationEvent,
    source: &str,
    session_id: &str,
    language: &str,
) -> serde_json::Value {
    let (title, body) = event_to_text(event, source, session_id, language);
    serde_json::json!({
        "msg_type": "interactive",
        "card": {
            "config": { "wide_screen_mode": false },
            "header": {
                "title": { "tag": "plain_text", "content": title },
                "template": event_color(event)
            },
            "elements": [
                {
                    "tag": "div",
                    "text": { "tag": "lark_md", "content": body }
                }
            ]
        }
    })
}

fn event_color(event: &NotificationEvent) -> &'static str {
    match event {
        NotificationEvent::Error { .. } => "red",
        NotificationEvent::Completion { .. } => "green",
        NotificationEvent::SessionStart => "blue",
        NotificationEvent::SessionStop => "grey",
        NotificationEvent::WaitingApproval { .. }
        | NotificationEvent::WaitingInput { .. }
        | NotificationEvent::PlanApproval { .. } => "orange",
        _ => "turquoise",
    }
}

#[derive(Clone, Copy)]
struct Labels {
    session_started: &'static str,
    session_started_body: &'static str,
    session_ended: &'static str,
    session_ended_body: &'static str,
    tool: &'static str,
    using_tool: &'static str,
    task_complete: &'static str,
    task_completed: &'static str,
    error: &'static str,
    error_body: &'static str,
    needs_approval: &'static str,
    needs_approval_body: &'static str,
    needs_input: &'static str,
    needs_input_body: &'static str,
    plan_approval: &'static str,
    plan_approval_body: &'static str,
}

fn labels(language: &str) -> Labels {
    match language {
        lang if lang.starts_with("zh") => Labels {
            session_started: "会话开始",
            session_started_body: "会话已开始",
            session_ended: "会话结束",
            session_ended_body: "会话已结束",
            tool: "工具",
            using_tool: "正在使用工具",
            task_complete: "任务完成",
            task_completed: "任务已完成",
            error: "发生错误",
            error_body: "会话发生错误",
            needs_approval: "需要审批",
            needs_approval_body: "会话需要审批",
            needs_input: "需要回答",
            needs_input_body: "会话需要回答",
            plan_approval: "计划审批",
            plan_approval_body: "会话需要计划审批",
        },
        lang if lang.starts_with("ja") => Labels {
            session_started: "セッション開始",
            session_started_body: "セッションが開始されました",
            session_ended: "セッション終了",
            session_ended_body: "セッションが終了しました",
            tool: "ツール",
            using_tool: "ツールを使用中",
            task_complete: "タスク完了",
            task_completed: "タスクが完了しました",
            error: "エラー",
            error_body: "セッションでエラーが発生しました",
            needs_approval: "承認待ち",
            needs_approval_body: "セッションで承認が必要です",
            needs_input: "入力待ち",
            needs_input_body: "セッションで入力が必要です",
            plan_approval: "計画承認",
            plan_approval_body: "セッションで計画承認が必要です",
        },
        lang if lang.starts_with("ko") => Labels {
            session_started: "세션 시작",
            session_started_body: "세션이 시작되었습니다",
            session_ended: "세션 종료",
            session_ended_body: "세션이 종료되었습니다",
            tool: "도구",
            using_tool: "도구 사용 중",
            task_complete: "작업 완료",
            task_completed: "작업이 완료되었습니다",
            error: "오류",
            error_body: "세션에서 오류가 발생했습니다",
            needs_approval: "승인 필요",
            needs_approval_body: "세션에서 승인이 필요합니다",
            needs_input: "입력 필요",
            needs_input_body: "세션에서 입력이 필요합니다",
            plan_approval: "계획 승인",
            plan_approval_body: "세션에서 계획 승인이 필요합니다",
        },
        lang if lang.starts_with("tr") => Labels {
            session_started: "Oturum başladı",
            session_started_body: "Oturum başladı",
            session_ended: "Oturum bitti",
            session_ended_body: "Oturum bitti",
            tool: "Araç",
            using_tool: "Araç kullanılıyor",
            task_complete: "Görev tamamlandı",
            task_completed: "Görev tamamlandı",
            error: "Hata",
            error_body: "Oturumda hata oluştu",
            needs_approval: "Onay gerekli",
            needs_approval_body: "Oturumda onay gerekli",
            needs_input: "Girdi gerekli",
            needs_input_body: "Oturumda girdi gerekli",
            plan_approval: "Plan onayı",
            plan_approval_body: "Oturumda plan onayı gerekli",
        },
        _ => Labels {
            session_started: "Session started",
            session_started_body: "Session started",
            session_ended: "Session ended",
            session_ended_body: "Session ended",
            tool: "Tool",
            using_tool: "Using tool",
            task_complete: "Task complete",
            task_completed: "Task completed",
            error: "Error",
            error_body: "Error in session",
            needs_approval: "Needs approval",
            needs_approval_body: "Needs approval in session",
            needs_input: "Needs input",
            needs_input_body: "Needs input in session",
            plan_approval: "Plan approval",
            plan_approval_body: "Plan approval needed in session",
        },
    }
}

fn event_to_text(
    event: &NotificationEvent,
    source: &str,
    session_id: &str,
    language: &str,
) -> (String, String) {
    let short_id = session_id
        .char_indices()
        .nth(8)
        .map_or(session_id, |(end, _)| &session_id[..end]);
    let labels = labels(language);

    match event {
        NotificationEvent::SessionStart => (
            format!("[{}] {}", source, labels.session_started),
            format!(
                "**[{}]** {} `{}`",
                source, labels.session_started_body, short_id
            ),
        ),
        NotificationEvent::SessionStop => (
            format!("[{}] {}", source, labels.session_ended),
            format!(
                "**[{}]** {} `{}`",
                source, labels.session_ended_body, short_id
            ),
        ),
        NotificationEvent::ToolUse { tool_name } => (
            format!("[{}] {}: {}", source, labels.tool, tool_name),
            format!(
                "**[{}]** {} `{}` `{}`",
                source, labels.using_tool, tool_name, short_id
            ),
        ),
        NotificationEvent::Completion { summary } => (
            format!("[{}] {}", source, labels.task_complete),
            format!(
                "**[{}]** {}\n\n> {}",
                source, labels.task_completed, summary
            ),
        ),
        NotificationEvent::Error { message } => (
            format!("[{}] {}", source, labels.error),
            format!(
                "**[{}]** {} `{}`\n\n> {}",
                source, labels.error_body, short_id, message
            ),
        ),
        NotificationEvent::WaitingApproval { tool_name, detail } => {
            let detail = detail.as_deref().filter(|value| !value.trim().is_empty());
            let body = match detail {
                Some(detail) => format!(
                    "**[{}]** {} `{}`\n\n{}",
                    source, labels.needs_approval_body, short_id, detail
                ),
                None => format!(
                    "**[{}]** {} `{}`\n\n> {}",
                    source, labels.needs_approval_body, short_id, tool_name
                ),
            };
            (format!("[{}] {}", source, labels.needs_approval), body)
        }
        NotificationEvent::WaitingInput { question } => (
            format!("[{}] {}", source, labels.needs_input),
            format!(
                "**[{}]** {} `{}`\n\n> {}",
                source, labels.needs_input_body, short_id, question
            ),
        ),
        NotificationEvent::PlanApproval { title } => (
            format!("[{}] {}", source, labels.plan_approval),
            format!(
                "**[{}]** {} `{}`\n\n> {}",
                source, labels.plan_approval_body, short_id, title
            ),
        ),
        NotificationEvent::Custom { title, body } => (title.clone(), body.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_id_preview_keeps_unicode_characters_intact() {
        let (_, body) = event_to_text(
            &NotificationEvent::SessionStart,
            "Agent",
            "任务编号🙂甲乙丙丁",
            "en",
        );
        assert!(body.ends_with("`任务编号🙂甲乙丙`"));
    }
}
