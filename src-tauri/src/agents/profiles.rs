use super::hook_manager;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub(super) const MARKER_PREFIX: &str = "AgentBro managed integration";

#[derive(Clone, Copy)]
pub enum HookEntryTemplate {
    Plain,
    Matcher(&'static str),
}

#[derive(Clone, Copy)]
pub struct HookEventDescriptor {
    pub name: &'static str,
    pub template: HookEntryTemplate,
    pub timeout: Option<u64>,
}

#[derive(Clone, Copy)]
pub enum JsonHookEntry {
    CommandOnly,
    TypedCommand,
}

#[derive(Clone, Copy)]
pub enum InstallationKind {
    JsonHooks { entry: JsonHookEntry, nested: bool },
    YamlHooks,
    CursorSettings,
    KiroAgentFile,
    ExecutableHookFiles,
    PluginFile,
    PluginDirectory,
    TomlHooks,
}

#[derive(Clone, Copy)]
pub struct AgentIntegrationProfile {
    pub id: &'static str,
    pub installation_kind: InstallationKind,
    pub configuration_path: &'static str,
    pub activation_path: Option<&'static str>,
    pub source: &'static str,
    pub extra_args: &'static [&'static str],
    pub events: &'static [HookEventDescriptor],
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookEventStatus {
    pub name: String,
    pub category: HookEventCategory,
    pub category_title: &'static str,
    pub category_subtitle: &'static str,
    pub timeout: Option<u64>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HookInstallHealth {
    Installed,
    NotInstalled,
    NeedsReinstall,
    SettingsCorrupted,
    Error,
}

impl HookInstallHealth {
    pub fn as_status_str(self) -> &'static str {
        match self {
            Self::Installed => "installed",
            Self::NotInstalled => "not_installed",
            Self::NeedsReinstall => "needs_reinstall",
            Self::SettingsCorrupted => "settings_corrupted",
            Self::Error => "error",
        }
    }

    pub fn is_present(self) -> bool {
        !matches!(self, Self::NotInstalled)
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum HookEventCategory {
    Approvals,
    Notifications,
    Lifecycle,
    Activity,
}

impl HookEventCategory {
    pub fn title(self) -> &'static str {
        match self {
            Self::Approvals => "审批",
            Self::Notifications => "通知",
            Self::Lifecycle => "生命周期",
            Self::Activity => "活动追踪",
        }
    }

    pub fn subtitle(self) -> &'static str {
        match self {
            Self::Approvals => "工具调用审批与权限请求，可能需要用户回应",
            Self::Notifications => "用户提示与通知事件",
            Self::Lifecycle => "会话开始/结束与子任务事件",
            Self::Activity => "工具完成、压缩等后台事件",
        }
    }

    pub fn for_event_name(name: &str) -> Self {
        match name {
            "PreToolUse" | "PermissionRequest" | "PermissionDenied" | "pre_tool_use"
            | "preToolUse" | "permissionRequest" => Self::Approvals,
            "Notification"
            | "UserPromptSubmit"
            | "user_prompt_submit"
            | "userPromptSubmit"
            | "userPromptSubmitted" => Self::Notifications,
            "SessionStart" | "SessionEnd" | "Stop" | "StopFailure" | "SubagentStart"
            | "SubagentStop" | "SubagentEnd" | "session_start" | "session_end" | "sessionStart"
            | "sessionEnd" | "agentSpawn" => Self::Lifecycle,
            _ => Self::Activity,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredHookSelections(BTreeMap<String, Vec<String>>);

pub const BASIC_AGENT_EVENTS: &[HookEventDescriptor] = &[
    plain_event("PreToolUse"),
    plain_event("PostToolUse"),
    plain_event("Notification"),
    plain_event("Stop"),
];

pub const SESSION_TOOL_EVENTS: &[HookEventDescriptor] = &[
    plain_event("SessionStart"),
    plain_event("PreToolUse"),
    plain_event("PostToolUse"),
    plain_event("Notification"),
    plain_event("Stop"),
];

pub const CODEBUDDY_EVENTS: &[HookEventDescriptor] = &[
    plain_event("PreToolUse"),
    plain_event("PostToolUse"),
    plain_event("SessionStart"),
    plain_event("SessionEnd"),
];

pub const GEMINI_EVENTS: &[HookEventDescriptor] = &[
    matcher_event("SessionStart", "*", None),
    matcher_event("SessionEnd", "*", None),
    matcher_event("BeforeTool", "*", Some(21_600)),
    matcher_event("AfterTool", "*", None),
    matcher_event("BeforeAgent", "*", None),
    matcher_event("AfterAgent", "*", None),
];

pub const COPILOT_EVENTS: &[HookEventDescriptor] = &[
    plain_event("agentStop"),
    plain_event("errorOccurred"),
    plain_event("notification"),
    plain_event("permissionRequest"),
    plain_event("postToolUseFailure"),
    plain_event("preCompact"),
    plain_event("preToolUse"),
    plain_event("sessionStart"),
    plain_event("sessionEnd"),
    plain_event("subagentStart"),
    plain_event("subagentStop"),
    plain_event("userPromptSubmitted"),
    plain_event("postToolUse"),
];

pub const CLINE_EVENTS: &[HookEventDescriptor] = &[
    HookEventDescriptor {
        name: "UserPromptSubmit",
        template: HookEntryTemplate::Plain,
        timeout: Some(5),
    },
    HookEventDescriptor {
        name: "PreToolUse",
        template: HookEntryTemplate::Plain,
        timeout: Some(5),
    },
    HookEventDescriptor {
        name: "PostToolUse",
        template: HookEntryTemplate::Plain,
        timeout: Some(5),
    },
    HookEventDescriptor {
        name: "TaskStart",
        template: HookEntryTemplate::Plain,
        timeout: Some(5),
    },
    HookEventDescriptor {
        name: "TaskResume",
        template: HookEntryTemplate::Plain,
        timeout: Some(5),
    },
    HookEventDescriptor {
        name: "TaskCancel",
        template: HookEntryTemplate::Plain,
        timeout: Some(5),
    },
    HookEventDescriptor {
        name: "TaskComplete",
        template: HookEntryTemplate::Plain,
        timeout: Some(5),
    },
    HookEventDescriptor {
        name: "PreCompact",
        template: HookEntryTemplate::Plain,
        timeout: Some(5),
    },
];

pub const SNAKE_SESSION_TOOL_EVENTS: &[HookEventDescriptor] = &[
    plain_event("pre_tool_use"),
    plain_event("post_tool_use"),
    plain_event("session_start"),
    plain_event("session_end"),
];

pub const OPENCODE_EVENTS: &[HookEventDescriptor] = &[
    plain_event("SessionStart"),
    plain_event("SessionEnd"),
    plain_event("UserPromptSubmit"),
    plain_event("PreToolUse"),
    plain_event("PostToolUse"),
    plain_event("PostToolUseFailure"),
    plain_event("PermissionRequest"),
    plain_event("Stop"),
    plain_event("SubagentStart"),
    plain_event("SubagentStop"),
    plain_event("ShellExecutionStart"),
    plain_event("ShellExecutionEnd"),
    plain_event("Error"),
    plain_event("TokenUsage"),
    plain_event("AgentThought"),
    plain_event("Notification"),
];

pub const HERMES_EVENTS: &[HookEventDescriptor] = &[
    plain_event("SessionStart"),
    plain_event("SessionEnd"),
    plain_event("UserPromptSubmit"),
    plain_event("PreToolUse"),
    plain_event("PostToolUse"),
    plain_event("PostToolUseFailure"),
    plain_event("Notification"),
    plain_event("Stop"),
];

pub const KIRO_EVENTS: &[HookEventDescriptor] = &[
    HookEventDescriptor {
        name: "agentSpawn",
        template: HookEntryTemplate::Plain,
        timeout: Some(10_000),
    },
    HookEventDescriptor {
        name: "userPromptSubmit",
        template: HookEntryTemplate::Plain,
        timeout: Some(10_000),
    },
    HookEventDescriptor {
        name: "preToolUse",
        template: HookEntryTemplate::Plain,
        timeout: Some(10_000),
    },
    HookEventDescriptor {
        name: "postToolUse",
        template: HookEntryTemplate::Plain,
        timeout: Some(10_000),
    },
    HookEventDescriptor {
        name: "stop",
        template: HookEntryTemplate::Plain,
        timeout: Some(10_000),
    },
];

pub const CLAUDE_CODE_EVENTS: &[HookEventDescriptor] = &[
    plain_event("UserPromptSubmit"),
    matcher_event("PreToolUse", "*", None),
    matcher_event("PostToolUse", "*", None),
    matcher_event("PostToolUseFailure", "*", None),
    matcher_event("PermissionRequest", "*", Some(21_600)),
    matcher_event("PermissionDenied", "*", None),
    matcher_event("Notification", "*", None),
    plain_event("Stop"),
    plain_event("SubagentStart"),
    plain_event("SubagentStop"),
    plain_event("SessionStart"),
    plain_event("SessionEnd"),
    plain_event("PreCompact"),
    plain_event("PostCompact"),
];

pub const CODEX_EVENTS: &[HookEventDescriptor] = &[
    HookEventDescriptor {
        name: "SessionStart",
        template: HookEntryTemplate::Plain,
        timeout: Some(5),
    },
    HookEventDescriptor {
        name: "UserPromptSubmit",
        template: HookEntryTemplate::Plain,
        timeout: Some(5),
    },
    HookEventDescriptor {
        name: "PreToolUse",
        template: HookEntryTemplate::Plain,
        timeout: Some(5),
    },
    HookEventDescriptor {
        name: "PostToolUse",
        template: HookEntryTemplate::Plain,
        timeout: Some(5),
    },
    HookEventDescriptor {
        name: "PostToolUseFailure",
        template: HookEntryTemplate::Plain,
        timeout: Some(5),
    },
    HookEventDescriptor {
        name: "PermissionRequest",
        template: HookEntryTemplate::Plain,
        timeout: Some(21_600),
    },
    HookEventDescriptor {
        name: "PermissionDenied",
        template: HookEntryTemplate::Plain,
        timeout: Some(5),
    },
    HookEventDescriptor {
        name: "Stop",
        template: HookEntryTemplate::Plain,
        timeout: Some(5),
    },
    HookEventDescriptor {
        name: "SessionEnd",
        template: HookEntryTemplate::Plain,
        timeout: Some(5),
    },
];

pub const QWEN_EVENTS: &[HookEventDescriptor] = &[
    plain_event("UserPromptSubmit"),
    matcher_event("PreToolUse", "*", None),
    matcher_event("PostToolUse", "*", None),
    matcher_event("PostToolUseFailure", "*", None),
    matcher_event("PermissionRequest", "*", Some(21_600)),
    matcher_event("Notification", "*", None),
    plain_event("Stop"),
    plain_event("SessionStart"),
    plain_event("SessionEnd"),
    plain_event("PreCompact"),
    plain_event("SubagentStart"),
    plain_event("SubagentStop"),
];

// Hook events for Kimi CLI. Schema reference:
// https://www.kimi.com/code/docs/kimi-code-cli/customization/hooks.html
//
// Matcher rules per the official docs:
// - matcher is a REGEX string (not a glob); empty string = match everything.
// - `"*"` is invalid regex (quantifier without leading element) and would
//   trigger a parse error or silent no-match — never use it here.
// - PreToolUse / PostToolUse / PostToolUseFailure match against tool name.
// - SessionStart matches source (startup/resume), SessionEnd matches reason.
// - SubagentStart / SubagentStop match against agent name.
// - PreCompact / PostCompact match against trigger.
// - Notification matches sink name.
// - UserPromptSubmit and Stop do not use a matcher.
// - StopFailure matches error type.
//
// AgentBro registers all hooks with empty matcher (=match all) so events
// always reach the bridge; finer filtering happens in the parser.
pub const KIMI_EVENTS: &[HookEventDescriptor] = &[
    HookEventDescriptor {
        name: "UserPromptSubmit",
        template: HookEntryTemplate::Plain,
        timeout: None,
    },
    HookEventDescriptor {
        name: "PreToolUse",
        template: HookEntryTemplate::Matcher(""),
        timeout: None,
    },
    HookEventDescriptor {
        name: "PostToolUse",
        template: HookEntryTemplate::Matcher(""),
        timeout: None,
    },
    HookEventDescriptor {
        name: "PostToolUseFailure",
        template: HookEntryTemplate::Matcher(""),
        timeout: None,
    },
    HookEventDescriptor {
        name: "Notification",
        template: HookEntryTemplate::Matcher(""),
        timeout: None,
    },
    HookEventDescriptor {
        name: "Stop",
        template: HookEntryTemplate::Plain,
        timeout: None,
    },
    HookEventDescriptor {
        name: "StopFailure",
        template: HookEntryTemplate::Matcher(""),
        timeout: None,
    },
    HookEventDescriptor {
        name: "SessionStart",
        template: HookEntryTemplate::Plain,
        timeout: None,
    },
    HookEventDescriptor {
        name: "SessionEnd",
        template: HookEntryTemplate::Plain,
        timeout: None,
    },
    HookEventDescriptor {
        name: "SubagentStart",
        template: HookEntryTemplate::Matcher(""),
        timeout: None,
    },
    HookEventDescriptor {
        name: "SubagentStop",
        template: HookEntryTemplate::Matcher(""),
        timeout: None,
    },
    HookEventDescriptor {
        name: "PreCompact",
        template: HookEntryTemplate::Matcher(""),
        timeout: None,
    },
    HookEventDescriptor {
        name: "PostCompact",
        template: HookEntryTemplate::Matcher(""),
        timeout: None,
    },
];

pub const QODER_EVENTS: &[HookEventDescriptor] = &[
    plain_event("UserPromptSubmit"),
    matcher_event("PreToolUse", "*", None),
    matcher_event("PostToolUse", "*", None),
    matcher_event("PostToolUseFailure", "*", None),
    matcher_event("PermissionRequest", "*", None),
    matcher_event("Notification", "*", None),
    plain_event("Stop"),
    plain_event("SessionStart"),
    plain_event("SessionEnd"),
    plain_event("PreCompact"),
    plain_event("SubagentStart"),
    plain_event("SubagentStop"),
];

const fn plain_event(name: &'static str) -> HookEventDescriptor {
    HookEventDescriptor {
        name,
        template: HookEntryTemplate::Plain,
        timeout: None,
    }
}

const fn matcher_event(
    name: &'static str,
    matcher: &'static str,
    timeout: Option<u64>,
) -> HookEventDescriptor {
    HookEventDescriptor {
        name,
        template: HookEntryTemplate::Matcher(matcher),
        timeout,
    }
}

pub fn claude_code_profile() -> AgentIntegrationProfile {
    AgentIntegrationProfile {
        id: "claude-code",
        installation_kind: InstallationKind::JsonHooks {
            entry: JsonHookEntry::TypedCommand,
            nested: true,
        },
        configuration_path: ".claude/settings.json",
        activation_path: None,
        source: "claude-code",
        extra_args: &[],
        events: CLAUDE_CODE_EVENTS,
    }
}

pub fn cline_profile() -> AgentIntegrationProfile {
    AgentIntegrationProfile {
        id: "cline",
        installation_kind: InstallationKind::ExecutableHookFiles,
        configuration_path: "Documents/Cline/Hooks",
        activation_path: None,
        source: "cline",
        extra_args: &[],
        events: CLINE_EVENTS,
    }
}

pub fn codex_profile() -> AgentIntegrationProfile {
    AgentIntegrationProfile {
        id: "codex",
        installation_kind: InstallationKind::JsonHooks {
            entry: JsonHookEntry::TypedCommand,
            nested: true,
        },
        configuration_path: ".codex/hooks.json",
        activation_path: Some(".codex/config.toml"),
        source: "codex",
        extra_args: &[],
        events: CODEX_EVENTS,
    }
}

pub fn antigravity_profile() -> AgentIntegrationProfile {
    json_profile(
        "antigravity",
        ".antigravity/settings.json",
        BASIC_AGENT_EVENTS,
    )
}

pub fn codebuddy_profile() -> AgentIntegrationProfile {
    json_profile("codebuddy", ".codebuddy/settings.json", CODEBUDDY_EVENTS)
}

pub fn codebuddycn_profile() -> AgentIntegrationProfile {
    json_profile_with_source(
        "codebuddycn",
        ".codybuddycn/settings.json",
        "codybuddycn",
        SESSION_TOOL_EVENTS,
    )
}

pub fn copilot_profile() -> AgentIntegrationProfile {
    command_only_json_profile(
        "copilot",
        ".config/github-copilot/hooks.json",
        COPILOT_EVENTS,
    )
}

pub fn cursor_profile() -> AgentIntegrationProfile {
    AgentIntegrationProfile {
        id: "cursor",
        installation_kind: InstallationKind::CursorSettings,
        configuration_path: ".cursor/settings.json",
        activation_path: None,
        source: "cursor",
        extra_args: &[],
        events: &[],
    }
}

pub fn cursor_cli_profile() -> AgentIntegrationProfile {
    json_profile("cursor-cli", ".cursor/settings.json", BASIC_AGENT_EVENTS)
}

pub fn droid_profile() -> AgentIntegrationProfile {
    json_profile("droid", ".factory/settings.json", BASIC_AGENT_EVENTS)
}

pub fn gemini_profile() -> AgentIntegrationProfile {
    AgentIntegrationProfile {
        id: "gemini",
        installation_kind: InstallationKind::JsonHooks {
            entry: JsonHookEntry::TypedCommand,
            nested: true,
        },
        configuration_path: ".gemini/settings.json",
        activation_path: None,
        source: "gemini",
        extra_args: &[],
        events: GEMINI_EVENTS,
    }
}

pub fn kiro_profile() -> AgentIntegrationProfile {
    AgentIntegrationProfile {
        id: "kiro",
        installation_kind: InstallationKind::KiroAgentFile,
        configuration_path: ".kiro/agents/agentbro.json",
        activation_path: None,
        source: "kiro",
        extra_args: &[],
        events: KIRO_EVENTS,
    }
}

pub fn pi_profile() -> AgentIntegrationProfile {
    json_profile("pi", ".pi/hooks.json", BASIC_AGENT_EVENTS)
}

pub fn qoder_profile() -> AgentIntegrationProfile {
    AgentIntegrationProfile {
        id: "qoder",
        installation_kind: InstallationKind::JsonHooks {
            entry: JsonHookEntry::TypedCommand,
            nested: true,
        },
        configuration_path: ".qoder/settings.json",
        activation_path: None,
        source: "qoder",
        extra_args: &[],
        events: QODER_EVENTS,
    }
}

pub fn qoder_cli_profile() -> AgentIntegrationProfile {
    json_profile("qoder-cli", ".qoder/settings.json", BASIC_AGENT_EVENTS)
}

pub fn qwen_profile() -> AgentIntegrationProfile {
    AgentIntegrationProfile {
        id: "qwen",
        installation_kind: InstallationKind::JsonHooks {
            entry: JsonHookEntry::TypedCommand,
            nested: true,
        },
        configuration_path: ".qwen/settings.json",
        activation_path: None,
        source: "qwen",
        extra_args: &[],
        events: QWEN_EVENTS,
    }
}

pub fn stepfun_profile() -> AgentIntegrationProfile {
    json_profile("stepfun", ".stepfun/settings.json", BASIC_AGENT_EVENTS)
}

pub fn workbuddy_profile() -> AgentIntegrationProfile {
    json_profile("workbuddy", ".workbuddy/hooks.json", BASIC_AGENT_EVENTS)
}

fn json_profile(
    id: &'static str,
    configuration_path: &'static str,
    events: &'static [HookEventDescriptor],
) -> AgentIntegrationProfile {
    AgentIntegrationProfile {
        id,
        installation_kind: InstallationKind::JsonHooks {
            entry: JsonHookEntry::TypedCommand,
            nested: false,
        },
        configuration_path,
        activation_path: None,
        source: id,
        extra_args: &[],
        events,
    }
}

fn json_profile_with_source(
    id: &'static str,
    configuration_path: &'static str,
    source: &'static str,
    events: &'static [HookEventDescriptor],
) -> AgentIntegrationProfile {
    AgentIntegrationProfile {
        id,
        installation_kind: InstallationKind::JsonHooks {
            entry: JsonHookEntry::TypedCommand,
            nested: false,
        },
        configuration_path,
        activation_path: None,
        source,
        extra_args: &[],
        events,
    }
}

fn command_only_json_profile(
    id: &'static str,
    configuration_path: &'static str,
    events: &'static [HookEventDescriptor],
) -> AgentIntegrationProfile {
    AgentIntegrationProfile {
        id,
        installation_kind: InstallationKind::JsonHooks {
            entry: JsonHookEntry::CommandOnly,
            nested: false,
        },
        configuration_path,
        activation_path: None,
        source: id,
        extra_args: &[],
        events,
    }
}

pub fn hermes_profile() -> AgentIntegrationProfile {
    AgentIntegrationProfile {
        id: "hermes",
        installation_kind: InstallationKind::PluginDirectory,
        configuration_path: ".hermes/plugins/agentbro",
        activation_path: None,
        source: "hermes",
        extra_args: &[],
        events: HERMES_EVENTS,
    }
}

pub fn opencode_profile() -> AgentIntegrationProfile {
    AgentIntegrationProfile {
        id: "opencode",
        installation_kind: InstallationKind::PluginFile,
        configuration_path: ".config/opencode/plugins/agentbro.js",
        activation_path: Some(".config/opencode/opencode.json"),
        source: "opencode",
        extra_args: &[],
        events: OPENCODE_EVENTS,
    }
}

pub fn kimi_profile() -> AgentIntegrationProfile {
    AgentIntegrationProfile {
        id: "kimi",
        installation_kind: InstallationKind::TomlHooks,
        configuration_path: ".kimi/config.toml",
        activation_path: None,
        source: "kimi",
        extra_args: &[],
        events: KIMI_EVENTS,
    }
}

pub fn deepseek_profile() -> AgentIntegrationProfile {
    AgentIntegrationProfile {
        id: "deepseek",
        installation_kind: InstallationKind::TomlHooks,
        configuration_path: ".deepseek/config.toml",
        activation_path: None,
        source: "deepseek",
        extra_args: &[],
        events: KIMI_EVENTS,
    }
}

pub fn configuration_url(profile: &AgentIntegrationProfile) -> PathBuf {
    resolve_home_path(profile.configuration_path)
}

pub fn activation_url(profile: &AgentIntegrationProfile) -> Option<PathBuf> {
    profile.activation_path.map(resolve_home_path)
}

pub fn profile_for_agent(id: &str) -> Option<AgentIntegrationProfile> {
    match id {
        "claude-code" => Some(claude_code_profile()),
        "cline" => Some(cline_profile()),
        "codex" => Some(codex_profile()),
        "antigravity" => Some(antigravity_profile()),
        "codebuddy" => Some(codebuddy_profile()),
        "codebuddycn" | "codybuddycn" => Some(codebuddycn_profile()),
        "copilot" => Some(copilot_profile()),
        "cursor" => Some(cursor_profile()),
        "cursor-cli" => Some(cursor_cli_profile()),
        "deepseek" => Some(deepseek_profile()),
        "droid" | "factory-droid" => Some(droid_profile()),
        "gemini" => Some(gemini_profile()),
        "hermes" => Some(hermes_profile()),
        "kiro" => Some(kiro_profile()),
        "kimi" => Some(kimi_profile()),
        "opencode" => Some(opencode_profile()),
        "pi" => Some(pi_profile()),
        "qoder" => Some(qoder_profile()),
        "qoder-cli" => Some(qoder_cli_profile()),
        "qwen" => Some(qwen_profile()),
        "stepfun" => Some(stepfun_profile()),
        "workbuddy" => Some(workbuddy_profile()),
        _ => None,
    }
}

pub fn supports_event_selection(profile: &AgentIntegrationProfile) -> bool {
    !profile.events.is_empty()
}

pub fn event_statuses(profile: &AgentIntegrationProfile) -> Vec<HookEventStatus> {
    let enabled = selected_event_name_set(profile);
    profile
        .events
        .iter()
        .map(|event| {
            let category = HookEventCategory::for_event_name(event.name);
            HookEventStatus {
                name: event.name.to_string(),
                category,
                category_title: category.title(),
                category_subtitle: category.subtitle(),
                timeout: event.timeout,
                enabled: enabled.contains(event.name),
            }
        })
        .collect()
}

pub fn selected_event_names(profile: &AgentIntegrationProfile) -> Vec<String> {
    selected_event_name_set(profile).into_iter().collect()
}

pub fn effective_event_descriptors(profile: &AgentIntegrationProfile) -> Vec<HookEventDescriptor> {
    effective_events(profile)
}

pub fn save_event_selection(
    profile: &AgentIntegrationProfile,
    enabled_event_names: &[String],
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    if !supports_event_selection(profile) {
        return Ok(Vec::new());
    }

    let valid_names: BTreeSet<&str> = profile.events.iter().map(|event| event.name).collect();
    let filtered: Vec<String> = enabled_event_names
        .iter()
        .filter(|name| valid_names.contains(name.as_str()))
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    if filtered.is_empty() {
        return Err("At least one hook event must be enabled".into());
    }

    let path = hook_selection_path();
    let mut stored = load_stored_hook_selections().0;
    stored.insert(profile.id.to_string(), filtered.clone());

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        path,
        serde_json::to_string_pretty(&StoredHookSelections(stored))?,
    )?;
    Ok(filtered)
}

pub fn install(profile: &AgentIntegrationProfile) -> Result<(), Box<dyn std::error::Error>> {
    install_at(profile, &configuration_url(profile))
}

pub fn install_at(
    profile: &AgentIntegrationProfile,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    hook_manager::ensure_bridge_binary()?;
    match profile.installation_kind {
        InstallationKind::JsonHooks { nested: true, .. } => {
            let command = managed_bridge_command(profile)?;
            update_nested_json_hooks(profile, path, &command)?;
        }
        InstallationKind::JsonHooks { nested: false, .. } => {
            update_json_hooks(profile, path)?;
        }
        InstallationKind::YamlHooks => {
            update_yaml_hooks(profile, path)?;
        }
        InstallationKind::CursorSettings => {
            update_cursor_settings(profile, path)?;
        }
        InstallationKind::KiroAgentFile => {
            write_kiro_agent_file(profile, path)?;
        }
        InstallationKind::ExecutableHookFiles => {
            write_executable_hook_files(profile, path)?;
        }
        InstallationKind::PluginFile => {
            write_plugin_file(profile, path)?;
            if let Some(activation_path) = activation_url(profile) {
                set_plugin_enabled(&activation_path, path, true)?;
            }
        }
        InstallationKind::PluginDirectory => {
            write_plugin_directory(profile, path)?;
        }
        InstallationKind::TomlHooks => {
            update_toml_hooks(profile, path)?;
        }
    }
    Ok(())
}

fn install_at_labeled(
    profile: &AgentIntegrationProfile,
    path: &Path,
    engine_label: Option<&str>,
    config_root: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    hook_manager::ensure_bridge_binary()?;
    let command = managed_bridge_command_labeled(profile, engine_label, config_root)?;
    match profile.installation_kind {
        InstallationKind::JsonHooks { nested: true, .. } => {
            update_nested_json_hooks(profile, path, &command)?;
        }
        InstallationKind::JsonHooks { nested: false, .. } => {
            let InstallationKind::JsonHooks {
                entry,
                nested: false,
            } = profile.installation_kind
            else {
                unreachable!();
            };
            let mut settings = hook_manager::read_json_config(path);
            if !settings.get("hooks").is_some_and(|hooks| hooks.is_object()) {
                settings["hooks"] = serde_json::json!({});
            }
            strip_json_hooks_for_profile(&mut settings, profile);
            let hooks = settings["hooks"].as_object_mut().expect("hooks is object");
            for event in effective_events(profile) {
                let value = hooks
                    .entry(event.name.to_string())
                    .or_insert_with(|| serde_json::json!([]));
                if !value.is_array() {
                    *value = serde_json::json!([]);
                }
                let entries = value.as_array_mut().expect("event hook list is array");
                entries.retain(|entry| !json_hook_contains_profile(entry, profile));
                entries.push(flat_json_hook_entry(entry, &event, &command));
            }
            hook_manager::write_json_config(path, &settings)?;
        }
        _ => {
            install_at(profile, path)?;
        }
    }
    Ok(())
}

pub fn install_custom_at(
    profile: &AgentIntegrationProfile,
    base_directory: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    install_custom_at_labeled(profile, base_directory, None, None)
}

pub fn install_custom_at_labeled(
    profile: &AgentIntegrationProfile,
    base_directory: &Path,
    engine_label: Option<&str>,
    config_root: Option<&str>,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let target = custom_installation_path(profile, base_directory);
    let has_label = engine_label.is_some() || config_root.is_some();
    if matches!(profile.installation_kind, InstallationKind::PluginFile) {
        hook_manager::ensure_bridge_binary()?;
        if has_label {
            write_plugin_file_labeled(profile, &target, engine_label, config_root)?;
        } else {
            write_plugin_file(profile, &target)?;
        }
        if let Some(activation_path) = custom_activation_path(profile, base_directory) {
            set_plugin_enabled(&activation_path, &target, true)?;
        }
    } else if has_label {
        install_at_labeled(profile, &target, engine_label, config_root)?;
    } else {
        install_at(profile, &target)?;
    }
    Ok(target)
}

pub fn custom_installation_path(
    profile: &AgentIntegrationProfile,
    base_directory: &Path,
) -> PathBuf {
    let file_name = Path::new(profile.configuration_path)
        .file_name()
        .map(|name| name.to_owned())
        .unwrap_or_else(|| std::ffi::OsString::from(profile.id));

    match profile.installation_kind {
        InstallationKind::PluginDirectory => {
            if base_directory
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == ".hermes")
            {
                return base_directory.join("plugins").join(file_name);
            }
            if base_directory
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == "plugins")
            {
                return base_directory.join(file_name);
            }
            base_directory.join(file_name)
        }
        _ => base_directory.join(file_name),
    }
}

fn custom_activation_path(
    profile: &AgentIntegrationProfile,
    base_directory: &Path,
) -> Option<PathBuf> {
    profile.activation_path?;
    if matches!(profile.installation_kind, InstallationKind::PluginFile) {
        let activation_file = Path::new(profile.activation_path?).file_name()?.to_owned();
        if base_directory
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "plugins")
        {
            return base_directory
                .parent()
                .map(|parent| parent.join(activation_file));
        }
        return Some(base_directory.join(activation_file));
    }
    None
}

pub fn uninstall(profile: &AgentIntegrationProfile) -> Result<(), Box<dyn std::error::Error>> {
    uninstall_at(profile, &configuration_url(profile))
}

pub fn uninstall_at(
    profile: &AgentIntegrationProfile,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    match profile.installation_kind {
        InstallationKind::JsonHooks { nested: true, .. } => {
            remove_nested_json_hooks(profile, path)?;
        }
        InstallationKind::JsonHooks { nested: false, .. } => {
            remove_json_hooks(profile, path)?;
        }
        InstallationKind::YamlHooks => {
            hook_manager::remove_hooks_yaml(path)?;
        }
        InstallationKind::CursorSettings => {
            remove_cursor_settings(path)?;
        }
        InstallationKind::KiroAgentFile => {
            if path.exists() {
                std::fs::remove_file(path)?;
            }
        }
        InstallationKind::ExecutableHookFiles => {
            remove_executable_hook_files(profile, path)?;
        }
        InstallationKind::PluginFile => {
            if contains_marker(path, profile) {
                let _ = std::fs::remove_file(path);
            }
            if let Some(activation_path) = activation_url(profile) {
                set_plugin_enabled(&activation_path, path, false)?;
            }
        }
        InstallationKind::PluginDirectory => {
            if contains_marker(path, profile) {
                let _ = std::fs::remove_dir_all(path);
            }
        }
        InstallationKind::TomlHooks => {
            remove_toml_hooks(path)?;
        }
    }
    Ok(())
}

pub fn is_installed(profile: &AgentIntegrationProfile) -> bool {
    is_installed_at(profile, &configuration_url(profile))
}

pub fn is_installed_at(profile: &AgentIntegrationProfile, path: &Path) -> bool {
    match profile.installation_kind {
        InstallationKind::JsonHooks { .. }
        | InstallationKind::YamlHooks
        | InstallationKind::KiroAgentFile => hook_manager::has_agentbro_hooks(path),
        InstallationKind::ExecutableHookFiles => executable_hook_files_installed(profile, path),
        InstallationKind::CursorSettings => cursor_settings_enabled(path),
        InstallationKind::PluginFile => {
            let has_plugin = contains_marker(path, profile);
            let enabled = activation_url(profile)
                .map(|activation_path| is_plugin_enabled(&activation_path, path))
                .unwrap_or(true);
            has_plugin && enabled
        }
        InstallationKind::PluginDirectory => contains_marker(path, profile),
        InstallationKind::TomlHooks => std::fs::read_to_string(path)
            .map(|content| super::toml_hooks::contains_managed(&content))
            .unwrap_or(false),
    }
}

pub fn install_health(profile: &AgentIntegrationProfile, path: &Path) -> HookInstallHealth {
    if !path.exists() {
        return HookInstallHealth::NotInstalled;
    }

    match profile.installation_kind {
        InstallationKind::JsonHooks { nested, .. } => json_hooks_health(profile, path, nested),
        InstallationKind::YamlHooks | InstallationKind::TomlHooks => {
            text_hooks_health(profile, path)
        }
        InstallationKind::CursorSettings => cursor_settings_health(profile, path),
        InstallationKind::KiroAgentFile => kiro_agent_file_health(profile, path),
        InstallationKind::ExecutableHookFiles => executable_hook_files_health(profile, path),
        InstallationKind::PluginFile => plugin_file_health(profile, path),
        InstallationKind::PluginDirectory => plugin_directory_health(profile, path),
    }
}

pub fn install_health_for_profile(profile: &AgentIntegrationProfile) -> HookInstallHealth {
    install_health(profile, &configuration_url(profile))
}

fn expected_command(profile: &AgentIntegrationProfile) -> Result<String, HookInstallHealth> {
    managed_bridge_command(profile).map_err(|_| HookInstallHealth::Error)
}

fn agentbro_command_is_current(profile: &AgentIntegrationProfile, command: &str) -> bool {
    if !is_agentbro_command(command) {
        return false;
    }

    let bridge = hook_manager::bridge_binary_path();
    let bridge_path = bridge.display().to_string();
    if !command.contains(&bridge_path) {
        return false;
    }

    if hook_manager::endpoint_env_assignments()
        .iter()
        .any(|assignment| !command.contains(assignment))
    {
        return false;
    }

    profile.id == "claude-code" || command_source_matches(command, profile.source)
}

fn command_source_matches(command: &str, source: &str) -> bool {
    let mut previous_was_source = false;
    for token in command.split_whitespace() {
        let token = shell_token_value(token);
        if previous_was_source {
            if token == source {
                return true;
            }
            previous_was_source = false;
        }

        if token == "--source" {
            previous_was_source = true;
            continue;
        }

        if let Some(value) = token.strip_prefix("--source=") {
            if shell_token_value(value) == source {
                return true;
            }
        }
    }
    false
}

fn shell_token_value(token: &str) -> &str {
    token.trim_matches(|c| c == '\'' || c == '"')
}

fn bridge_health() -> Option<HookInstallHealth> {
    if hook_manager::bridge_binary_is_current() {
        return None;
    }
    if hook_manager::ensure_bridge_binary().is_ok() && hook_manager::bridge_binary_is_current() {
        return None;
    }
    Some(HookInstallHealth::NeedsReinstall)
}

fn read_json_health(path: &Path) -> Result<Value, HookInstallHealth> {
    let content =
        std::fs::read_to_string(path).map_err(|_| HookInstallHealth::SettingsCorrupted)?;
    serde_json::from_str(&content).map_err(|_| HookInstallHealth::SettingsCorrupted)
}

fn json_hooks_health(
    profile: &AgentIntegrationProfile,
    path: &Path,
    nested: bool,
) -> HookInstallHealth {
    let settings = match read_json_health(path) {
        Ok(value) => value,
        Err(status) => return status,
    };
    let Some(hooks) = settings.get("hooks").and_then(Value::as_object) else {
        return if json_hook_contains_profile(&settings, profile) {
            HookInstallHealth::SettingsCorrupted
        } else {
            HookInstallHealth::NotInstalled
        };
    };

    if !json_hook_contains_profile(&settings, profile) {
        return HookInstallHealth::NotInstalled;
    }
    if let Some(status) = bridge_health() {
        return status;
    }

    if effective_events(profile)
        .iter()
        .any(|event| !json_event_has_current_profile_command(hooks, profile, event))
    {
        return HookInstallHealth::NeedsReinstall;
    }

    let mut commands = Vec::new();
    collect_json_agentbro_commands(&settings, &mut commands);
    if commands
        .iter()
        .any(|candidate| agentbro_command_is_current(profile, candidate))
    {
        return HookInstallHealth::Installed;
    }

    if nested
        && hooks.values().any(|entries| {
            entries.as_array().is_some_and(|entries| {
                entries.iter().any(|entry| {
                    json_hook_contains_profile(entry, profile)
                        && entry.get("hooks").and_then(Value::as_array).is_none()
                })
            })
        })
    {
        return HookInstallHealth::SettingsCorrupted;
    }

    HookInstallHealth::NeedsReinstall
}

fn json_event_has_current_profile_command(
    hooks: &serde_json::Map<String, Value>,
    profile: &AgentIntegrationProfile,
    event: &HookEventDescriptor,
) -> bool {
    let Some(entries) = hooks.get(event.name) else {
        return false;
    };
    let mut commands = Vec::new();
    collect_json_agentbro_commands(entries, &mut commands);
    commands
        .iter()
        .any(|candidate| agentbro_event_command_is_current(profile, event, candidate))
}

fn agentbro_event_command_is_current(
    profile: &AgentIntegrationProfile,
    event: &HookEventDescriptor,
    command: &str,
) -> bool {
    if !agentbro_command_is_current(profile, command) {
        return false;
    }

    if matches!(
        profile.installation_kind,
        InstallationKind::JsonHooks {
            entry: JsonHookEntry::CommandOnly,
            ..
        }
    ) {
        command_event_matches(command, event.name)
    } else {
        true
    }
}

fn command_event_matches(command: &str, event: &str) -> bool {
    let mut previous_was_event = false;
    for token in command.split_whitespace() {
        let token = shell_token_value(token);
        if previous_was_event {
            if token == event {
                return true;
            }
            previous_was_event = false;
        }

        if token == "--event" {
            previous_was_event = true;
            continue;
        }

        if let Some(value) = token.strip_prefix("--event=") {
            if shell_token_value(value) == event {
                return true;
            }
        }
    }
    false
}

fn collect_json_agentbro_commands(value: &Value, commands: &mut Vec<String>) {
    if let Some(entries) = value.as_array() {
        for entry in entries {
            collect_json_agentbro_commands(entry, commands);
        }
        return;
    }
    if let Some(command) = value.get("command").and_then(Value::as_str) {
        if is_agentbro_command(command) {
            commands.push(command.to_string());
        }
    }
    if let Some(hooks) = value.get("hooks").and_then(Value::as_array) {
        for hook in hooks {
            collect_json_agentbro_commands(hook, commands);
        }
    }
    if let Some(hooks) = value.get("hooks").and_then(Value::as_object) {
        for entries in hooks.values() {
            if let Some(entries) = entries.as_array() {
                for entry in entries {
                    collect_json_agentbro_commands(entry, commands);
                }
            }
        }
    }
}

fn text_hooks_health(profile: &AgentIntegrationProfile, path: &Path) -> HookInstallHealth {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => return HookInstallHealth::SettingsCorrupted,
    };
    let marker = marker(profile);
    if !content.contains(&marker) && !content.contains("agentbro-bridge") {
        return HookInstallHealth::NotInstalled;
    }
    if let Some(status) = bridge_health() {
        return status;
    }

    let command = match expected_command(profile) {
        Ok(command) => command,
        Err(status) => return status,
    };
    if !content.contains(&command) {
        return HookInstallHealth::NeedsReinstall;
    }
    if effective_events(profile)
        .iter()
        .any(|event| !content.contains(event.name))
    {
        return HookInstallHealth::NeedsReinstall;
    }

    HookInstallHealth::Installed
}

fn cursor_settings_health(profile: &AgentIntegrationProfile, path: &Path) -> HookInstallHealth {
    let settings = match read_json_health(path) {
        Ok(value) => value,
        Err(status) => return status,
    };
    let Some(hooks) = settings.get("agentBroHooks") else {
        return HookInstallHealth::NotInstalled;
    };
    if !hooks
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return HookInstallHealth::NotInstalled;
    }
    if let Some(status) = bridge_health() {
        return status;
    }
    let command = match expected_command(profile) {
        Ok(command) => command,
        Err(status) => return status,
    };
    match hooks.get("command").and_then(Value::as_str) {
        Some(candidate) if candidate == command => HookInstallHealth::Installed,
        Some(_) => HookInstallHealth::NeedsReinstall,
        None => HookInstallHealth::SettingsCorrupted,
    }
}

fn kiro_agent_file_health(profile: &AgentIntegrationProfile, path: &Path) -> HookInstallHealth {
    let settings = match read_json_health(path) {
        Ok(value) => value,
        Err(status) => return status,
    };
    if settings
        .get("_agentbro")
        .and_then(Value::as_str)
        .is_none_or(|value| value != marker(profile))
    {
        return HookInstallHealth::NotInstalled;
    }
    if let Some(status) = bridge_health() {
        return status;
    }

    let Some(hooks) = settings.get("hooks").and_then(Value::as_object) else {
        return HookInstallHealth::SettingsCorrupted;
    };
    let command = match expected_command(profile) {
        Ok(command) => command,
        Err(status) => return status,
    };
    for event in effective_events(profile) {
        let Some(candidate) = hooks
            .get(event.name)
            .and_then(|hook| hook.get("command"))
            .and_then(Value::as_str)
        else {
            return HookInstallHealth::NeedsReinstall;
        };
        if candidate != command {
            return HookInstallHealth::NeedsReinstall;
        }
    }

    HookInstallHealth::Installed
}

fn plugin_file_health(profile: &AgentIntegrationProfile, path: &Path) -> HookInstallHealth {
    let current = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => return HookInstallHealth::SettingsCorrupted,
    };
    if !current.contains(&marker(profile)) {
        return HookInstallHealth::NotInstalled;
    }
    if let Some(status) = bridge_health() {
        return status;
    }
    match opencode_plugin_source(profile) {
        Ok(expected) if current == expected => HookInstallHealth::Installed,
        Ok(_) => HookInstallHealth::NeedsReinstall,
        Err(_) => HookInstallHealth::Error,
    }
}

fn plugin_directory_health(profile: &AgentIntegrationProfile, path: &Path) -> HookInstallHealth {
    if !path.is_dir() {
        return HookInstallHealth::NotInstalled;
    }
    if !contains_marker(path, profile) {
        return HookInstallHealth::NotInstalled;
    }
    if let Some(status) = bridge_health() {
        return status;
    }

    let expected_files = match hermes_plugin_files(profile) {
        Ok(files) => files,
        Err(_) => return HookInstallHealth::Error,
    };
    for (name, expected) in expected_files {
        match std::fs::read_to_string(path.join(name)) {
            Ok(current) if current == expected => {}
            Ok(_) => return HookInstallHealth::NeedsReinstall,
            Err(_) => return HookInstallHealth::NeedsReinstall,
        }
    }
    HookInstallHealth::Installed
}

pub fn managed_bridge_command(
    profile: &AgentIntegrationProfile,
) -> Result<String, Box<dyn std::error::Error>> {
    managed_bridge_command_labeled(profile, None, None)
}

pub fn managed_bridge_command_labeled(
    profile: &AgentIntegrationProfile,
    engine_label: Option<&str>,
    config_root: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    let bridge = hook_manager::ensure_bridge_binary()?;
    let mut args = vec!["--source".to_string(), profile.source.to_string()];
    if cfg!(target_os = "windows") {
        if let Some(label) = engine_label {
            args.extend(["--engine-label".to_string(), label.to_string()]);
        }
        if let Some(root) = config_root {
            args.extend(["--config-root".to_string(), root.to_string()]);
        }
    }
    args.extend(profile.extra_args.iter().map(|arg| arg.to_string()));
    let mut parts = hook_manager::bridge_command_parts(&bridge, &args);
    if !cfg!(target_os = "windows") {
        if let Some(label) = engine_label {
            parts.insert(
                1,
                format!("AGENTBRO_ENGINE_LABEL={}", hook_manager::shell_quote(label)),
            );
        }
        if let Some(root) = config_root {
            parts.insert(
                if engine_label.is_some() { 2 } else { 1 },
                format!("AGENTBRO_CONFIG_ROOT={}", hook_manager::shell_quote(root)),
            );
        }
    }
    Ok(parts.join(" "))
}

fn bridge_args_json(
    profile: &AgentIntegrationProfile,
) -> Result<String, Box<dyn std::error::Error>> {
    let bridge = hook_manager::ensure_bridge_binary()?;
    let mut args = vec![
        bridge.display().to_string(),
        "--source".to_string(),
        profile.source.to_string(),
    ];
    args.extend(profile.extra_args.iter().map(|arg| arg.to_string()));
    Ok(serde_json::to_string(&args)?)
}

fn bridge_env_json() -> Result<String, Box<dyn std::error::Error>> {
    bridge_env_json_labeled(None, None)
}

fn bridge_env_json_labeled(
    engine_label: Option<&str>,
    config_root: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    let endpoint = crate::hook_endpoint::current();
    let mut env = serde_json::json!({
        crate::hook_endpoint::HOOK_SOCKET_ENV: endpoint.socket_path,
        crate::hook_endpoint::HOOK_PORT_ENV: endpoint.tcp_port.to_string(),
    });
    if let Some(label) = engine_label {
        env["AGENTBRO_ENGINE_LABEL"] = serde_json::Value::String(label.to_string());
    }
    if let Some(root) = config_root {
        env["AGENTBRO_CONFIG_ROOT"] = serde_json::Value::String(root.to_string());
    }
    Ok(serde_json::to_string(&env)?)
}

fn enabled_event_names_json(
    profile: &AgentIntegrationProfile,
) -> Result<String, Box<dyn std::error::Error>> {
    let events = event_names(&effective_events(profile));
    Ok(serde_json::to_string(&events)?)
}

fn marker(profile: &AgentIntegrationProfile) -> String {
    format!("{}: {}", MARKER_PREFIX, profile.id)
}

fn resolve_home_path(relative: &str) -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
    relative
        .split('/')
        .filter(|part| !part.is_empty())
        .fold(home, |path, part| path.join(part))
}

fn hook_selection_path() -> PathBuf {
    if let Ok(path) = std::env::var("AGENTBRO_HOOK_SELECTIONS_PATH") {
        return PathBuf::from(path);
    }
    let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
    home.join(".agentbro").join("hook-event-selections.json")
}

fn load_stored_hook_selections() -> StoredHookSelections {
    std::fs::read_to_string(hook_selection_path())
        .ok()
        .and_then(|content| serde_json::from_str::<StoredHookSelections>(&content).ok())
        .unwrap_or_else(|| StoredHookSelections(BTreeMap::new()))
}

fn selected_event_name_set(profile: &AgentIntegrationProfile) -> BTreeSet<String> {
    if !supports_event_selection(profile) {
        return BTreeSet::new();
    }
    let valid_names: BTreeSet<String> = profile
        .events
        .iter()
        .map(|event| event.name.to_string())
        .collect();
    let stored_selections = load_stored_hook_selections();
    let Some(stored) = stored_selections.0.get(profile.id) else {
        return valid_names;
    };
    let filtered: BTreeSet<String> = stored
        .iter()
        .filter(|name| valid_names.contains(*name))
        .cloned()
        .collect();
    if filtered.is_empty() {
        valid_names
    } else {
        filtered
    }
}

fn effective_events(profile: &AgentIntegrationProfile) -> Vec<HookEventDescriptor> {
    let enabled = selected_event_name_set(profile);
    if enabled.is_empty() {
        return profile.events.to_vec();
    }
    profile
        .events
        .iter()
        .copied()
        .filter(|event| enabled.contains(event.name))
        .collect()
}

fn contains_marker(path: &Path, profile: &AgentIntegrationProfile) -> bool {
    let marker = marker(profile);
    if path.is_dir() {
        return ["plugin.yaml", "__init__.py"].iter().any(|name| {
            std::fs::read_to_string(path.join(name))
                .map(|s| s.contains(&marker))
                .unwrap_or(false)
        });
    }
    std::fs::read_to_string(path)
        .map(|s| s.contains(&marker))
        .unwrap_or(false)
}

fn update_json_hooks(
    profile: &AgentIntegrationProfile,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let InstallationKind::JsonHooks {
        entry,
        nested: false,
    } = profile.installation_kind
    else {
        return Err("Profile is not a flat JSON hooks integration".into());
    };
    let command = managed_bridge_command(profile)?;
    let mut settings = hook_manager::read_json_config(path);
    if !settings.get("hooks").is_some_and(|hooks| hooks.is_object()) {
        settings["hooks"] = serde_json::json!({});
    }
    strip_json_hooks_for_profile(&mut settings, profile);

    let hooks = settings["hooks"].as_object_mut().expect("hooks is object");
    for event in effective_events(profile) {
        let value = hooks
            .entry(event.name.to_string())
            .or_insert_with(|| serde_json::json!([]));
        if !value.is_array() {
            *value = serde_json::json!([]);
        }
        let entries = value.as_array_mut().expect("event hook list is array");
        entries.retain(|entry| !json_hook_contains_profile(entry, profile));
        let hook = flat_json_hook_entry(entry, &event, &command);
        if matches!(entry, JsonHookEntry::CommandOnly) {
            entries.insert(0, hook);
        } else {
            entries.push(hook);
        }
    }

    hook_manager::write_json_config(path, &settings)
}

fn remove_json_hooks(
    profile: &AgentIntegrationProfile,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(());
    }
    let mut settings = hook_manager::read_json_config(path);
    strip_json_hooks_for_profile(&mut settings, profile);
    hook_manager::write_json_config(path, &settings)
}

fn flat_json_hook_entry(entry: JsonHookEntry, event: &HookEventDescriptor, command: &str) -> Value {
    let command = match entry {
        JsonHookEntry::CommandOnly => command_for_event(command, event),
        JsonHookEntry::TypedCommand => command.to_string(),
    };
    let mut hook = match entry {
        JsonHookEntry::CommandOnly => serde_json::json!({ "command": command }),
        JsonHookEntry::TypedCommand => {
            serde_json::json!({ "type": "command", "command": command })
        }
    };
    if let Some(timeout) = event.timeout {
        hook["timeout"] = serde_json::json!(timeout);
    }
    hook
}

fn command_for_event(command: &str, event: &HookEventDescriptor) -> String {
    format!(
        "{} --event {}",
        command,
        hook_manager::shell_quote(event.name)
    )
}

fn update_nested_json_hooks(
    profile: &AgentIntegrationProfile,
    path: &Path,
    command: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    let mut settings = hook_manager::read_json_config(path);
    if !settings.get("hooks").is_some_and(|hooks| hooks.is_object()) {
        settings["hooks"] = serde_json::json!({});
    }
    strip_json_hooks_for_profile(&mut settings, profile);
    let hooks = settings["hooks"].as_object_mut().expect("hooks is object");

    for event in effective_events(profile) {
        let value = hooks
            .entry(event.name.to_string())
            .or_insert_with(|| serde_json::json!([]));
        if !value.is_array() {
            *value = serde_json::json!([]);
        }
        let groups = value.as_array_mut().expect("event hook groups is array");
        groups.retain(|group| !json_hook_contains_profile(group, profile));

        let mut inner_hook = serde_json::json!({
            "type": "command",
            "command": command,
        });
        if let Some(timeout) = event.timeout {
            inner_hook["timeout"] = serde_json::json!(timeout);
        }
        let mut group = serde_json::json!({ "hooks": [inner_hook] });
        if let HookEntryTemplate::Matcher(matcher) = event.template {
            group["matcher"] = serde_json::json!(matcher);
        }
        groups.push(group);
    }

    // For Gemini: bypass native permission prompts so AgentBro's BeforeTool
    // hook is the sole permission gate (avoids double prompts).
    if profile.id == "gemini" {
        if let Some(root) = settings.as_object_mut() {
            let security = root
                .entry("security".to_string())
                .or_insert_with(|| serde_json::json!({}));
            if let Some(sec_obj) = security.as_object_mut() {
                let permissions = sec_obj
                    .entry("permissions".to_string())
                    .or_insert_with(|| serde_json::json!({}));
                if let Some(perm_obj) = permissions.as_object_mut() {
                    perm_obj.insert("mode".to_string(), serde_json::json!("auto"));
                }
            }
        }
    }

    hook_manager::write_json_config(path, &settings)?;
    Ok(settings)
}

fn remove_nested_json_hooks(
    profile: &AgentIntegrationProfile,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(());
    }
    let mut settings = hook_manager::read_json_config(path);
    strip_json_hooks_for_profile(&mut settings, profile);
    hook_manager::write_json_config(path, &settings)
}

pub fn install_nested_json_hooks_at(
    profile: &AgentIntegrationProfile,
    path: &Path,
    command: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    hook_manager::ensure_bridge_binary()?;
    update_nested_json_hooks(profile, path, command)
}

#[cfg(test)]
fn json_hook_contains_agentbro(value: &Value) -> bool {
    value
        .get("command")
        .and_then(Value::as_str)
        .is_some_and(is_agentbro_command)
        || value
            .get("hooks")
            .and_then(Value::as_array)
            .is_some_and(|hooks| hooks.iter().any(json_hook_contains_agentbro))
        || value
            .get("hooks")
            .and_then(Value::as_object)
            .is_some_and(|hooks| {
                hooks.values().any(|entries| {
                    entries
                        .as_array()
                        .is_some_and(|entries| entries.iter().any(json_hook_contains_agentbro))
                })
            })
}

fn json_hook_contains_profile(value: &Value, profile: &AgentIntegrationProfile) -> bool {
    let mut commands = Vec::new();
    collect_json_agentbro_commands(value, &mut commands);
    commands
        .iter()
        .any(|command| agentbro_command_matches_profile(profile, command))
}

fn agentbro_command_matches_profile(profile: &AgentIntegrationProfile, command: &str) -> bool {
    if !is_agentbro_command(command) {
        return false;
    }

    profile.id == "claude-code" || command_source_matches(command, profile.source)
}

fn strip_json_hooks_for_profile(settings: &mut Value, profile: &AgentIntegrationProfile) {
    strip_json_hooks_matching(settings, |entry| json_hook_contains_profile(entry, profile));
}

#[cfg(test)]
fn strip_json_hooks(settings: &mut Value) {
    strip_json_hooks_matching(settings, json_hook_contains_agentbro);
}

fn strip_json_hooks_matching<F>(settings: &mut Value, mut should_remove: F)
where
    F: FnMut(&Value) -> bool,
{
    if let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) {
        for value in hooks.values_mut() {
            if let Some(entries) = value.as_array_mut() {
                entries.retain(|entry| !should_remove(entry));
            }
        }
        hooks.retain(|_, value| {
            value
                .as_array()
                .map(|entries| !entries.is_empty())
                .unwrap_or(true)
        });
    }
}

fn is_agentbro_command(command: &str) -> bool {
    command.contains("agentbro-bridge")
        || command.contains("/.agentbro/")
        || command.contains("AGENTBRO_")
        || command.contains(MARKER_PREFIX)
}

fn update_yaml_hooks(
    profile: &AgentIntegrationProfile,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let command = managed_bridge_command(profile)?;
    let events = event_names(&effective_events(profile));
    hook_manager::inject_hooks_yaml(path, &command, &events)
}

fn update_cursor_settings(
    profile: &AgentIntegrationProfile,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut settings = hook_manager::read_json_config(path);
    settings["agentBroHooks"] = serde_json::json!({
        "command": managed_bridge_command(profile)?,
        "enabled": true,
    });
    hook_manager::write_json_config(path, &settings)
}

fn remove_cursor_settings(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(());
    }
    let mut settings = hook_manager::read_json_config(path);
    if let Some(object) = settings.as_object_mut() {
        object.remove("agentBroHooks");
    }
    hook_manager::write_json_config(path, &settings)
}

fn cursor_settings_enabled(path: &Path) -> bool {
    hook_manager::read_json_config(path)
        .get("agentBroHooks")
        .and_then(|hooks| hooks.get("enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn write_kiro_agent_file(
    profile: &AgentIntegrationProfile,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let command = managed_bridge_command(profile)?;
    let hooks = effective_events(profile)
        .iter()
        .map(|event| {
            (
                event.name.to_string(),
                serde_json::json!({
                    "command": command,
                    "timeout_ms": event.timeout.unwrap_or(10_000),
                }),
            )
        })
        .collect::<serde_json::Map<String, Value>>();
    let content = serde_json::json!({
        "_agentbro": marker(profile),
        "name": "agentbro",
        "description": "AgentBro status monitor hooks",
        "hooks": hooks,
    });
    hook_manager::write_json_config(path, &content)
}

fn write_executable_hook_files(
    profile: &AgentIntegrationProfile,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(path)?;
    let effective = effective_events(profile);
    let effective_names = effective
        .iter()
        .map(|event| event.name)
        .collect::<BTreeSet<_>>();
    remove_disabled_executable_hook_files(profile, path, &effective_names)?;
    for event in effective {
        let file_path = path.join(event.name);
        std::fs::write(&file_path, executable_hook_script(profile, &event)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file_path, std::fs::Permissions::from_mode(0o755))?;
        }
    }
    Ok(())
}

fn remove_disabled_executable_hook_files(
    profile: &AgentIntegrationProfile,
    path: &Path,
    effective_names: &BTreeSet<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    for event in profile.events {
        if effective_names.contains(event.name) {
            continue;
        }
        let file_path = path.join(event.name);
        let Ok(content) = std::fs::read_to_string(&file_path) else {
            continue;
        };
        if content.contains(&marker(profile)) {
            let _ = std::fs::remove_file(file_path);
        }
    }
    Ok(())
}

fn executable_hook_script(
    profile: &AgentIntegrationProfile,
    event: &HookEventDescriptor,
) -> Result<String, Box<dyn std::error::Error>> {
    let bridge = hook_manager::ensure_bridge_binary()?;
    let mut args = vec![
        "--source".to_string(),
        profile.source.to_string(),
        "--event".to_string(),
        event.name.to_string(),
    ];
    args.extend(profile.extra_args.iter().map(|arg| arg.to_string()));
    let command = hook_manager::bridge_command_parts(&bridge, &args).join(" ");
    Ok(format!(
        "#!/bin/bash\n# {}\nINPUT=$(cat)\nprintf '%s' \"$INPUT\" | {} >/dev/null 2>&1 &\nprintf '{{\"cancel\":false}}'\n",
        marker(profile),
        command
    ))
}

fn remove_executable_hook_files(
    profile: &AgentIntegrationProfile,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    for event in profile.events {
        let file_path = path.join(event.name);
        let Ok(content) = std::fs::read_to_string(&file_path) else {
            continue;
        };
        if content.contains(&marker(profile)) {
            let _ = std::fs::remove_file(file_path);
        }
    }
    Ok(())
}

fn executable_hook_files_installed(profile: &AgentIntegrationProfile, path: &Path) -> bool {
    executable_hook_files_health(profile, path) == HookInstallHealth::Installed
}

fn executable_hook_files_health(
    profile: &AgentIntegrationProfile,
    path: &Path,
) -> HookInstallHealth {
    if !path.is_dir() {
        return HookInstallHealth::NotInstalled;
    }
    let expected_events = effective_events(profile);
    if expected_events.is_empty() {
        return HookInstallHealth::NotInstalled;
    }
    let expected_names = expected_events
        .iter()
        .map(|event| event.name)
        .collect::<BTreeSet<_>>();
    let mut saw_any_managed = false;
    for event in expected_events {
        let file_path = path.join(event.name);
        let Ok(content) = std::fs::read_to_string(&file_path) else {
            return if saw_any_managed {
                HookInstallHealth::NeedsReinstall
            } else {
                HookInstallHealth::NotInstalled
            };
        };
        if !content.contains(&marker(profile)) {
            return if saw_any_managed {
                HookInstallHealth::NeedsReinstall
            } else {
                HookInstallHealth::NotInstalled
            };
        }
        saw_any_managed = true;
        match executable_hook_script(profile, &event) {
            Ok(expected) if expected == content => {}
            Ok(_) => return HookInstallHealth::NeedsReinstall,
            Err(_) => return HookInstallHealth::Error,
        }
    }
    if has_disabled_managed_executable_hook_files(profile, path, &expected_names) {
        return HookInstallHealth::NeedsReinstall;
    }
    HookInstallHealth::Installed
}

fn has_disabled_managed_executable_hook_files(
    profile: &AgentIntegrationProfile,
    path: &Path,
    expected_names: &BTreeSet<&str>,
) -> bool {
    profile.events.iter().any(|event| {
        if expected_names.contains(event.name) {
            return false;
        }
        let file_path = path.join(event.name);
        std::fs::read_to_string(&file_path)
            .map(|content| content.contains(&marker(profile)))
            .unwrap_or(false)
    })
}

fn event_names(events: &[HookEventDescriptor]) -> Vec<&'static str> {
    events.iter().map(|event| event.name).collect()
}

fn write_plugin_file(
    profile: &AgentIntegrationProfile,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(parent) = path.parent() else {
        return Err("Plugin path has no parent directory".into());
    };
    std::fs::create_dir_all(parent)?;
    std::fs::write(path, opencode_plugin_source(profile)?)?;
    Ok(())
}

fn write_plugin_file_labeled(
    profile: &AgentIntegrationProfile,
    path: &Path,
    engine_label: Option<&str>,
    config_root: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(parent) = path.parent() else {
        return Err("Plugin path has no parent directory".into());
    };
    std::fs::create_dir_all(parent)?;
    let marker = marker(profile);
    let args_json = bridge_args_json(profile)?;
    let env_json = bridge_env_json_labeled(engine_label, config_root)?;
    let enabled_events_json = enabled_event_names_json(profile)?;
    let source =
        opencode_plugin_source_from_parts(&marker, &args_json, &env_json, &enabled_events_json)?;
    std::fs::write(path, source)?;
    Ok(())
}

fn write_plugin_directory(
    profile: &AgentIntegrationProfile,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(path)?;
    for (name, content) in hermes_plugin_files(profile)? {
        std::fs::write(path.join(name), content)?;
    }
    Ok(())
}

fn set_plugin_enabled(
    activation_path: &Path,
    plugin_path: &Path,
    enabled: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut json = hook_manager::read_json_config(activation_path);
    let plugin_specifier = format!("file://{}", plugin_path.display());
    let plugin_path_string = plugin_path.display().to_string();
    let existing = json
        .get("plugin")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut plugins: Vec<Value> = existing
        .into_iter()
        .filter(|entry| {
            let Some(value) = entry.as_str() else {
                return true;
            };
            value != plugin_specifier && value != plugin_path_string
        })
        .collect();

    if enabled {
        plugins.push(Value::String(plugin_specifier));
    }

    if plugins.is_empty() {
        if let Some(object) = json.as_object_mut() {
            object.remove("plugin");
        }
    } else {
        json["plugin"] = Value::Array(plugins);
    }

    hook_manager::write_json_config(activation_path, &json)
}

fn is_plugin_enabled(activation_path: &Path, plugin_path: &Path) -> bool {
    let json = hook_manager::read_json_config(activation_path);
    let plugin_specifier = format!("file://{}", plugin_path.display());
    let plugin_path_string = plugin_path.display().to_string();
    json.get("plugin")
        .and_then(Value::as_array)
        .map(|plugins| {
            plugins.iter().any(|entry| {
                entry
                    .as_str()
                    .map(|value| value == plugin_specifier || value == plugin_path_string)
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn build_managed_hook_entries(
    profile: &AgentIntegrationProfile,
    command: &str,
) -> Vec<super::toml_hooks::TomlHookEntry> {
    effective_events(profile)
        .into_iter()
        .map(|event| {
            let matcher = match event.template {
                HookEntryTemplate::Plain => None,
                HookEntryTemplate::Matcher(m) => Some(m.to_string()),
            };
            super::toml_hooks::TomlHookEntry {
                event: event.name.to_string(),
                command: command.to_string(),
                matcher,
                timeout: event.timeout,
            }
        })
        .collect()
}

fn update_toml_hooks(
    profile: &AgentIntegrationProfile,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let existing_raw = std::fs::read_to_string(path).unwrap_or_default();
    let pre_cleaned = strip_legacy_sentinel_block(&existing_raw);

    if let Some((line, trigger)) = super::toml_hooks::detect_conflicting_hooks_key(&pre_cleaned) {
        return Err(format!(
            "Cannot install {} hooks: existing `{}` at {}:{} conflicts with the [[hooks]] array-of-tables form required by AgentBro. Remove or rename it manually, then try again.",
            profile.id,
            trigger,
            path.display(),
            line,
        )
        .into());
    }

    let command = managed_bridge_command(profile)?;
    let managed = build_managed_hook_entries(profile, &command);
    let segments = super::toml_hooks::parse_segments(&pre_cleaned);
    let output = super::toml_hooks::rebuild(&segments, &managed, &marker(profile));

    // mtime guard: skip the write when the file is already byte-identical.
    // This avoids triggering the hook recovery worker (`hooks/recovery.rs`)
    // into a reinstall loop on settings-file change notifications.
    if existing_raw == output {
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, output)?;
    Ok(())
}

fn remove_toml_hooks(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(());
    }
    let existing_raw = std::fs::read_to_string(path)?;
    let pre_cleaned = strip_legacy_sentinel_block(&existing_raw);
    let segments = super::toml_hooks::parse_segments(&pre_cleaned);
    // marker placeholder is irrelevant when managed list is empty.
    let output = super::toml_hooks::rebuild(&segments, &[], MARKER_PREFIX);
    if existing_raw == output {
        return Ok(());
    }
    std::fs::write(path, output)?;
    Ok(())
}

fn strip_legacy_sentinel_block(content: &str) -> String {
    let mut result = String::new();
    let mut inside = false;
    for line in content.lines() {
        match line.trim() {
            "# [AGENTBRO-START]" => {
                inside = true;
                continue;
            }
            "# [AGENTBRO-END]" => {
                inside = false;
                continue;
            }
            _ if !inside => {
                result.push_str(line);
                result.push('\n');
            }
            _ => {}
        }
    }
    result
}

fn hermes_plugin_files(
    profile: &AgentIntegrationProfile,
) -> Result<Vec<(&'static str, String)>, Box<dyn std::error::Error>> {
    let marker = marker(profile);
    let args_json = bridge_args_json(profile)?;
    let env_json = bridge_env_json()?;
    let enabled_events_json = enabled_event_names_json(profile)?;
    let plugin_yaml = format!(
        r#"# {marker}
name: agentbro
version: 1.0.0
description: Forward Hermes Agent plugin hooks to AgentBro
provides_hooks:
  - on_session_start
  - pre_llm_call
  - pre_tool_call
  - post_tool_call
  - post_llm_call
  - on_session_end
  - on_session_finalize
  - on_session_reset
"#
    );
    let init_py = format!(
        r#"# {marker}
"""AgentBro Hermes plugin.

Generated by AgentBro. Reinstall from settings if you need to refresh it.
"""

from __future__ import annotations

import json
import os
import subprocess
import threading

BRIDGE_ARGS = json.loads(r'''{args_json}''')
BRIDGE_ENV = json.loads(r'''{env_json}''')
ENABLED_EVENTS = set(json.loads(r'''{enabled_events_json}'''))
ENV_KEYS = ["TERM_PROGRAM", "ITERM_SESSION_ID", "TERM_SESSION_ID", "TMUX", "TMUX_PANE", "KITTY_WINDOW_ID", "__CFBundleIdentifier", "CMUX_WORKSPACE_ID", "CMUX_SURFACE_ID", "CMUX_SOCKET_PATH"]
_SESSION_STATE = {{}}

def _stable_text(value):
    if value is None:
        return None
    if isinstance(value, str):
        stripped = value.strip()
        return stripped or None
    try:
        return json.dumps(value, ensure_ascii=False)
    except Exception:
        return None

def _payload_session_id(raw_value):
    normalized = _stable_text(raw_value)
    if not normalized:
        return None
    return normalized if normalized.startswith("hermes-") else f"hermes-{{normalized}}"

def _collect_env():
    return {{key: value for key in ENV_KEYS if (value := os.environ.get(key))}}

def _detect_tty():
    for fd in (0, 1, 2):
        try:
            value = os.ttyname(fd)
        except OSError:
            continue
        if value:
            return value
    return os.environ.get("TTY")

def _state_for(session_id):
    if session_id not in _SESSION_STATE:
        _SESSION_STATE[session_id] = {{"cwd": None, "last_user": None, "last_assistant": None, "did_emit_start": False}}
    return _SESSION_STATE[session_id]

def _resolve_session_id(*candidates, **kwargs):
    for candidate in candidates:
        resolved = _payload_session_id(candidate)
        if resolved:
            return resolved
    for key in ("session_id", "task_id", "conversation_id"):
        resolved = _payload_session_id(kwargs.get(key))
        if resolved:
            return resolved
    return None

def _resolve_cwd(kwargs):
    for key in ("cwd", "working_directory", "directory"):
        resolved = _stable_text(kwargs.get(key))
        if resolved:
            return resolved
    try:
        return os.getcwd()
    except OSError:
        return None

def _extract_user_message(kwargs):
    for key in ("user_message", "prompt", "input", "query", "text", "content"):
        candidate = _stable_text(kwargs.get(key))
        if candidate:
            return candidate
    return None

def _extract_assistant_message(kwargs):
    for key in ("assistant_response", "response", "content", "text", "message"):
        candidate = kwargs.get(key)
        if isinstance(candidate, dict):
            candidate = candidate.get("content") or candidate.get("text") or candidate.get("message")
        normalized = _stable_text(candidate)
        if normalized:
            return normalized
    return None

def _bridge_payload(session_id, **payload):
    state = _state_for(session_id)
    cwd = payload.pop("cwd", None) or state.get("cwd")
    if cwd:
        state["cwd"] = cwd
    return {{"session_id": session_id, "_env": _collect_env(), "_tty": _detect_tty(), "cwd": cwd, **payload}}

def _spawn_bridge(payload):
    env = os.environ.copy()
    env.update(BRIDGE_ENV)
    bridged_env = payload.pop("_env", None)
    tty = payload.pop("_tty", None)
    if isinstance(bridged_env, dict):
        env.update({{key: value for key, value in bridged_env.items() if isinstance(value, str) and value}})
    if tty and not env.get("TTY"):
        env["TTY"] = tty
    try:
        process = subprocess.Popen(BRIDGE_ARGS, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, text=True, env=env, start_new_session=True)
        if process.stdin:
            process.stdin.write(json.dumps(payload, ensure_ascii=False))
            process.stdin.close()
    except Exception:
        return

def _emit(payload):
    if payload.get("hook_event_name") not in ENABLED_EVENTS:
        return
    threading.Thread(target=_spawn_bridge, args=(payload,), daemon=True).start()

def _emit_session_start(session_id, platform=None, model=None, message=None, cwd=None):
    state = _state_for(session_id)
    if state.get("did_emit_start"):
        return
    _emit(_bridge_payload(session_id, hook_event_name="SessionStart", platform=_stable_text(platform), model=_stable_text(model), message=_stable_text(message), cwd=cwd))
    state["did_emit_start"] = True

def _on_session_start(session_id=None, platform=None, model=None, **kwargs):
    resolved_id = _resolve_session_id(session_id, **kwargs)
    if resolved_id:
        _emit_session_start(resolved_id, platform=platform, model=model, message=_extract_user_message(kwargs), cwd=_resolve_cwd(kwargs))

def _on_pre_llm_call(session_id=None, user_message=None, **kwargs):
    resolved_id = _resolve_session_id(session_id, kwargs.get("task_id"), **kwargs)
    if not resolved_id:
        return None
    prompt = _stable_text(user_message) or _extract_user_message(kwargs)
    cwd = _resolve_cwd(kwargs)
    _emit_session_start(resolved_id, platform=kwargs.get("platform"), model=kwargs.get("model"), message=prompt, cwd=cwd)
    if prompt:
        _state_for(resolved_id)["last_user"] = prompt
    _emit(_bridge_payload(resolved_id, hook_event_name="UserPromptSubmit", prompt=prompt, cwd=cwd))
    return None

def _on_pre_tool_call(tool_name=None, args=None, task_id=None, **kwargs):
    resolved_id = _resolve_session_id(task_id, kwargs.get("session_id"), **kwargs)
    if resolved_id:
        _emit(_bridge_payload(resolved_id, hook_event_name="PreToolUse", tool_name=_stable_text(tool_name) or "Tool", tool_input=args if isinstance(args, dict) else None, cwd=_resolve_cwd(kwargs)))

def _on_post_tool_call(tool_name=None, args=None, result=None, task_id=None, **kwargs):
    resolved_id = _resolve_session_id(task_id, kwargs.get("session_id"), **kwargs)
    if resolved_id:
        error_text = None
        if isinstance(result, dict):
            error_text = _stable_text(result.get("error"))
        _emit(_bridge_payload(resolved_id, hook_event_name="PostToolUseFailure" if error_text else "PostToolUse", tool_name=_stable_text(tool_name) or "Tool", tool_input=args if isinstance(args, dict) else None, error=error_text, cwd=_resolve_cwd(kwargs)))

def _on_post_llm_call(session_id=None, assistant_response=None, **kwargs):
    resolved_id = _resolve_session_id(session_id, kwargs.get("task_id"), **kwargs)
    if not resolved_id:
        return
    reply = _stable_text(assistant_response) or _extract_assistant_message(kwargs)
    if reply:
        _state_for(resolved_id)["last_assistant"] = reply
        _emit(_bridge_payload(resolved_id, hook_event_name="Notification", notification_type="assistant_message", message=reply, cwd=_resolve_cwd(kwargs)))

def _on_session_end(session_id=None, completed=None, interrupted=None, platform=None, model=None, **kwargs):
    resolved_id = _resolve_session_id(session_id, kwargs.get("task_id"), **kwargs)
    if resolved_id:
        assistant = _state_for(resolved_id).get("last_assistant") or _extract_assistant_message(kwargs)
        _emit(_bridge_payload(resolved_id, hook_event_name="Stop", last_assistant_message=assistant, platform=_stable_text(platform), model=_stable_text(model), completed=bool(completed), interrupted=bool(interrupted), cwd=_resolve_cwd(kwargs)))

def _on_session_finalize(session_id=None, platform=None, model=None, **kwargs):
    resolved_id = _resolve_session_id(session_id, kwargs.get("task_id"), **kwargs)
    if resolved_id:
        assistant = _state_for(resolved_id).get("last_assistant") or _extract_assistant_message(kwargs)
        _emit(_bridge_payload(resolved_id, hook_event_name="SessionEnd", last_assistant_message=assistant, platform=_stable_text(platform), model=_stable_text(model), completed=True, interrupted=False, cwd=_resolve_cwd(kwargs)))
        _SESSION_STATE.pop(resolved_id, None)

def _on_session_reset(session_id=None, platform=None, model=None, **kwargs):
    resolved_id = _resolve_session_id(session_id, kwargs.get("task_id"), **kwargs)
    if resolved_id:
        _emit_session_start(resolved_id, platform=platform, model=model, message=_extract_user_message(kwargs), cwd=_resolve_cwd(kwargs))

def register(ctx):
    ctx.register_hook("on_session_start", _on_session_start)
    ctx.register_hook("pre_llm_call", _on_pre_llm_call)
    ctx.register_hook("pre_tool_call", _on_pre_tool_call)
    ctx.register_hook("post_tool_call", _on_post_tool_call)
    ctx.register_hook("post_llm_call", _on_post_llm_call)
    ctx.register_hook("on_session_end", _on_session_end)
    ctx.register_hook("on_session_finalize", _on_session_finalize)
    ctx.register_hook("on_session_reset", _on_session_reset)
"#
    );

    Ok(vec![("plugin.yaml", plugin_yaml), ("__init__.py", init_py)])
}

fn opencode_plugin_source(
    profile: &AgentIntegrationProfile,
) -> Result<String, Box<dyn std::error::Error>> {
    let marker = marker(profile);
    let args_json = bridge_args_json(profile)?;
    let env_json = bridge_env_json()?;
    let enabled_events_json = enabled_event_names_json(profile)?;
    opencode_plugin_source_from_parts(&marker, &args_json, &env_json, &enabled_events_json)
}

fn opencode_plugin_source_from_parts(
    marker: &str,
    args_json: &str,
    env_json: &str,
    enabled_events_json: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    Ok(format!(
        r#"// {marker}
// Generated by AgentBro. Reinstall from settings if you need to refresh it.

const BRIDGE_ARGS = {args_json};
const BRIDGE_ENV = {env_json};
const ENABLED_EVENTS = new Set({enabled_events_json});
const ENV_KEYS = ["TERM_PROGRAM", "ITERM_SESSION_ID", "TERM_SESSION_ID", "TMUX", "TMUX_PANE", "KITTY_WINDOW_ID", "__CFBundleIdentifier", "CMUX_WORKSPACE_ID", "CMUX_SURFACE_ID", "CMUX_SOCKET_PATH"];

function isObject(value) {{
  return value !== null && typeof value === "object" && !Array.isArray(value);
}}
function stableString(value) {{
  if (value == null) return undefined;
  if (typeof value === "string") {{
    const trimmed = value.trim();
    return trimmed.length > 0 ? trimmed : undefined;
  }}
  try {{ return JSON.stringify(value); }} catch {{ return undefined; }}
}}
function capitalize(value) {{
  if (typeof value !== "string" || value.length === 0) return undefined;
  return value.charAt(0).toUpperCase() + value.slice(1);
}}
function collectBridgeEnv() {{
  const collected = {{}};
  for (const key of ENV_KEYS) {{
    const value = process.env[key];
    if (typeof value === "string" && value.length > 0) collected[key] = value;
  }}
  return collected;
}}
function detectTTY() {{
  try {{
    const {{ execSync }} = require("child_process");
    let walkPid = process.pid;
    for (let index = 0; index < 8; index += 1) {{
      const info = execSync(`ps -o tty=,ppid= -p ${{walkPid}}`, {{ timeout: 1000 }}).toString().trim();
      if (!info) break;
      const parts = info.split(/\s+/);
      const tty = parts[0];
      const ppid = Number.parseInt(parts[1] || "", 10);
      if (tty && tty !== "??" && tty !== "?") return tty.startsWith("/dev/") ? tty : `/dev/${{tty}}`;
      if (!Number.isFinite(ppid) || ppid <= 1) break;
      walkPid = ppid;
    }}
  }} catch {{}}
  return undefined;
}}
function makeBasePayload(sessionId, extra = {{}}) {{
  return {{ session_id: sessionId, agent: "opencode", _source: "opencode", _env: collectBridgeEnv(), _tty: detectedTTY, pid: process.pid, ...extra }};
}}
function normalizeQuestions(questions) {{
  if (!Array.isArray(questions)) return [];
  return questions.map((question) => ({{
    id: stableString(question?.id),
    header: stableString(question?.header),
    question: stableString(question?.question),
    options: Array.isArray(question?.options) ? question.options.map((option) => typeof option === "string" ? {{ label: option }} : option).filter(Boolean) : [],
    multiple: question?.multiple === true
  }})).filter((question) => question.question);
}}
function questionAnswerArrays(questions, answers) {{
  if (!isObject(answers)) return [];
  return Object.values(answers).map((value) => Array.isArray(value) ? value.map(stableString).filter(Boolean) : [stableString(value)].filter(Boolean)).filter((entry) => entry.length > 0);
}}
function toolNameForPermission(permission) {{
  if (permission === "bash") return "Bash";
  if (permission === "edit" || permission === "write") return "Write";
  return capitalize(permission) ?? "Permission";
}}
function permissionToolInput(properties) {{
  const patterns = Array.isArray(properties?.patterns) ? properties.patterns.map(stableString).filter(Boolean) : [];
  const input = {{}};
  if (patterns.length > 0) input.patterns = patterns;
  if (isObject(properties?.metadata)) input.metadata = properties.metadata;
  if (properties?.permission === "bash" && patterns.length > 0) {{
    input.command = patterns.join(" && ");
  }} else if ((properties?.permission === "edit" || properties?.permission === "write") && patterns.length > 0) {{
    input.file_path = patterns[0];
  }} else if (properties?.permission) {{
    input.permission_type = properties.permission;
  }}
  return Object.keys(input).length > 0 ? input : {{ permission_type: properties?.permission ?? "unknown" }};
}}

const detectedTTY = detectTTY();
const sessions = new Map();
const messageRoles = new Map();
function getSession(sessionID) {{
  return sessions.get(sessionID);
}}
function ensureSession(sessionID) {{
  if (!sessions.has(sessionID)) sessions.set(sessionID, {{ cwd: undefined, lastAssistantText: "", pendingTitle: undefined, parentID: undefined, _started: false, _lastAssistantNotifyAt: 0 }});
  return sessions.get(sessionID);
}}
function resolveSessionId(rawSessionID) {{
  const session = sessions.get(rawSessionID);
  if (!session) return null;
  return session.parentID ? `opencode-${{session.parentID}}` : `opencode-${{rawSessionID}}`;
}}

function mapEvent(event) {{
  const type = event?.type;
  const properties = isObject(event?.properties) ? event.properties : {{}};
  if (typeof type !== "string") return undefined;

  if (type === "session.created" && isObject(properties.info) && stableString(properties.info.id)) {{
    const rawSessionID = stableString(properties.info.id);
    const cwd = stableString(properties.info.directory);
    const parentID = stableString(properties.info.parentID);
    const session = ensureSession(rawSessionID);
    session.cwd = cwd;
    session.parentID = parentID;
    session._started = true;
    if (parentID) {{
      return makeBasePayload(`opencode-${{parentID}}`, {{ hook_event_name: "SubagentStart", agent_name: `subagent-${{rawSessionID}}`, description: "Subagent started" }});
    }}
    return makeBasePayload(`opencode-${{rawSessionID}}`, {{ hook_event_name: "SessionStart", cwd }});
  }}
  if (type === "session.deleted" && isObject(properties.info) && stableString(properties.info.id)) {{
    const rawSessionID = stableString(properties.info.id);
    const session = getSession(rawSessionID);
    const hadParent = !!session?.parentID;
    const parentID = session?.parentID;
    sessions.delete(rawSessionID);
    if (hadParent && parentID) {{
      return makeBasePayload(`opencode-${{parentID}}`, {{ hook_event_name: "SubagentStop", cwd: session?.cwd, agent_name: `subagent-${{rawSessionID}}`, description: "Subagent removed" }});
    }}
    if (!hadParent) {{
      return makeBasePayload(`opencode-${{rawSessionID}}`, {{ hook_event_name: "SessionEnd" }});
    }}
    return undefined;
  }}
  if (type === "session.updated" && isObject(properties.info) && stableString(properties.info.id)) {{
    const rawSessionID = stableString(properties.info.id);
    const session = ensureSession(rawSessionID);
    const isNew = !session._started;
    const cwd = stableString(properties.info.directory) ?? session.cwd;
    if (cwd) session.cwd = cwd;
    const title = stableString(properties.info.title);
    if (title && !title.startsWith("New session")) session.pendingTitle = title;
    const parentID = stableString(properties.info.parentID);
    if (parentID !== undefined) session.parentID = parentID;
    if (properties.info.time?.archived && !session.parentID) return makeBasePayload(`opencode-${{rawSessionID}}`, {{ hook_event_name: "SessionEnd", cwd }});
    if (isNew) {{
      session._started = true;
      if (parentID) {{
        return makeBasePayload(`opencode-${{parentID}}`, {{ hook_event_name: "SubagentStart", agent_name: `subagent-${{rawSessionID}}`, description: "Subagent started" }});
      }}
      return makeBasePayload(`opencode-${{rawSessionID}}`, {{ hook_event_name: "SessionStart", cwd }});
    }}
    return undefined;
  }}
  if (type === "session.status" && stableString(properties.sessionID)) {{
    const rawSessionID = stableString(properties.sessionID);
    if (properties.status?.type === "idle") {{
      const session = getSession(rawSessionID);
      if (!session) return undefined;
      if (session.parentID) {{
        const parentSessionId = `opencode-${{session.parentID}}`;
        sessions.delete(rawSessionID);
        return makeBasePayload(parentSessionId, {{ hook_event_name: "SubagentStop", cwd: session.cwd, agent_name: `subagent-${{rawSessionID}}`, description: "Subagent completed" }});
      }}
      return makeBasePayload(`opencode-${{rawSessionID}}`, {{ hook_event_name: "Stop", cwd: session.cwd, last_assistant_message: session.lastAssistantText || undefined, session_title: session.pendingTitle || undefined }});
    }}
    return undefined;
  }}
  if (type === "message.updated" && isObject(properties.info) && stableString(properties.info.id) && stableString(properties.info.sessionID)) {{
    messageRoles.set(stableString(properties.info.id), {{ role: stableString(properties.info.role), sessionID: stableString(properties.info.sessionID) }});
    return undefined;
  }}
  if (type === "message.part.updated" && isObject(properties.part) && stableString(properties.part.sessionID)) {{
    const rawSessionID = stableString(properties.part.sessionID);
    const session = getSession(rawSessionID);
    if (!session) return undefined;
    const sessionId = resolveSessionId(rawSessionID);
    if (!sessionId) return undefined;
    if (properties.part.type === "text" && stableString(properties.part.messageID)) {{
      const meta = messageRoles.get(stableString(properties.part.messageID));
      const text = stableString(properties.part.text);
      if (!meta || !text) return undefined;
      if (meta.role === "user") return makeBasePayload(sessionId, {{ hook_event_name: "UserPromptSubmit", cwd: session.cwd, prompt: text }});
      if (meta.role === "assistant") {{
        if (text !== session.lastAssistantText) {{
          session.lastAssistantText = text;
          const now = Date.now();
          if (!session._lastAssistantNotifyAt || now - session._lastAssistantNotifyAt >= 1000) {{
            session._lastAssistantNotifyAt = now;
            return makeBasePayload(sessionId, {{ hook_event_name: "Notification", cwd: session.cwd, notification_type: "assistant_message", message: text }});
          }}
        }}
      }}
      return undefined;
    }}
    if (properties.part.type === "tool") {{
      const toolName = capitalize(stableString(properties.part.tool)) ?? "Tool";
      const toolInput = properties.part.state?.input;
      const state = stableString(properties.part.state?.status);
      if (state === "running" || state === "pending") return makeBasePayload(sessionId, {{ hook_event_name: "PreToolUse", cwd: session.cwd, tool_name: toolName, tool_input: isObject(toolInput) || Array.isArray(toolInput) ? toolInput : undefined, tool_state: state }});
      if (state === "completed" || state === "error") return makeBasePayload(sessionId, {{ hook_event_name: "PostToolUse", cwd: session.cwd, tool_name: toolName, tool_input: isObject(toolInput) || Array.isArray(toolInput) ? toolInput : undefined, tool_state: state }});
    }}
  }}
  if (type === "permission.asked" && stableString(properties.id) && stableString(properties.sessionID)) {{
    const rawSessionID = stableString(properties.sessionID);
    const session = getSession(rawSessionID);
    if (!session) return undefined;
    const sessionId = resolveSessionId(rawSessionID);
    if (!sessionId) return undefined;
    return makeBasePayload(sessionId, {{ hook_event_name: "PermissionRequest", cwd: session.cwd, tool_name: toolNameForPermission(properties.permission), tool_input: permissionToolInput(properties), _opencode_request_id: stableString(properties.id) }});
  }}
  if (type === "question.asked" && stableString(properties.id) && stableString(properties.sessionID)) {{
    const rawSessionID = stableString(properties.sessionID);
    const session = getSession(rawSessionID);
    if (!session) return undefined;
    const sessionId = resolveSessionId(rawSessionID);
    if (!sessionId) return undefined;
    return makeBasePayload(sessionId, {{ hook_event_name: "PermissionRequest", cwd: session.cwd, tool_name: "AskUserQuestion", tool_input: {{ questions: normalizeQuestions(properties.questions) }}, _opencode_request_id: stableString(properties.id) }});
  }}
  if (type === "session.error" && isObject(properties) && stableString(properties.sessionID)) {{
    const rawSessionID = stableString(properties.sessionID);
    const session = getSession(rawSessionID);
    if (!session) return undefined;
    const sessionId = resolveSessionId(rawSessionID);
    if (!sessionId) return undefined;
    const errorMsg = stableString(properties.error?.message) ?? stableString(properties.error) ?? "Session error";
    return makeBasePayload(sessionId, {{ hook_event_name: "Error", cwd: session.cwd, message: errorMsg }});
  }}
  if (type === "session.compacted" && stableString(properties.sessionID)) {{
    const rawSessionID = stableString(properties.sessionID);
    const session = getSession(rawSessionID);
    if (!session) return undefined;
    const sessionId = resolveSessionId(rawSessionID);
    if (!sessionId) return undefined;
    return makeBasePayload(sessionId, {{ hook_event_name: "Notification", cwd: session.cwd, message: "Context compacted" }});
  }}

  if (type === "session.next.step.ended" && stableString(properties.sessionID)) {{
    const rawSessionID = stableString(properties.sessionID);
    const session = getSession(rawSessionID);
    if (!session) return undefined;
    const sessionId = resolveSessionId(rawSessionID);
    if (!sessionId) return undefined;
    const tokens = isObject(properties.tokens) ? properties.tokens : {{}};
    return makeBasePayload(sessionId, {{ hook_event_name: "TokenUsage", cwd: session.cwd, input_tokens: tokens.input, output_tokens: tokens.output, cost: properties.cost }});
  }}
  if (type === "session.next.step.failed" && stableString(properties.sessionID)) {{
    const rawSessionID = stableString(properties.sessionID);
    const session = getSession(rawSessionID);
    if (!session) return undefined;
    const sessionId = resolveSessionId(rawSessionID);
    if (!sessionId) return undefined;
    const errorMsg = stableString(properties.error?.message) ?? "Step failed";
    return makeBasePayload(sessionId, {{ hook_event_name: "Error", cwd: session.cwd, message: errorMsg }});
  }}
  if (type === "session.next.shell.started" && stableString(properties.sessionID) && stableString(properties.command)) {{
    const rawSessionID = stableString(properties.sessionID);
    const session = getSession(rawSessionID);
    if (!session) return undefined;
    const sessionId = resolveSessionId(rawSessionID);
    if (!sessionId) return undefined;
    return makeBasePayload(sessionId, {{ hook_event_name: "ShellExecutionStart", cwd: session.cwd, command: properties.command }});
  }}
  if (type === "session.next.shell.ended" && stableString(properties.sessionID)) {{
    const rawSessionID = stableString(properties.sessionID);
    const session = getSession(rawSessionID);
    if (!session) return undefined;
    const sessionId = resolveSessionId(rawSessionID);
    if (!sessionId) return undefined;
    return makeBasePayload(sessionId, {{ hook_event_name: "ShellExecutionEnd", cwd: session.cwd, command: "", output: stableString(properties.output) }});
  }}
  if (type === "session.next.compaction.started" && stableString(properties.sessionID)) {{
    const rawSessionID = stableString(properties.sessionID);
    const session = getSession(rawSessionID);
    if (!session) return undefined;
    const sessionId = resolveSessionId(rawSessionID);
    if (!sessionId) return undefined;
    const reason = stableString(properties.reason);
    const reasonText = (reason && reason.length > 0) ? reason : "auto";
    return makeBasePayload(sessionId, {{ hook_event_name: "Notification", cwd: session.cwd, message: `Compacting context (${{reasonText}})` }});
  }}
  if (type === "session.next.compaction.ended" && stableString(properties.sessionID)) {{
    const rawSessionID = stableString(properties.sessionID);
    const session = getSession(rawSessionID);
    if (!session) return undefined;
    const sessionId = resolveSessionId(rawSessionID);
    if (!sessionId) return undefined;
    return makeBasePayload(sessionId, {{ hook_event_name: "Notification", cwd: session.cwd, message: "Context compaction complete" }});
  }}
  if (type === "session.next.tool.failed" && stableString(properties.sessionID)) {{
    const rawSessionID = stableString(properties.sessionID);
    const session = getSession(rawSessionID);
    if (!session) return undefined;
    const sessionId = resolveSessionId(rawSessionID);
    if (!sessionId) return undefined;
    const errorMsg = stableString(properties.error?.message) ?? "Tool failed";
    const callID = stableString(properties.callID) ?? "unknown";
    return makeBasePayload(sessionId, {{ hook_event_name: "PostToolUseFailure", cwd: session.cwd, tool_name: `tool:${{callID}}`, message: errorMsg }});
  }}
  if (type === "session.next.reasoning.ended" && stableString(properties.sessionID) && stableString(properties.text)) {{
    const rawSessionID = stableString(properties.sessionID);
    const session = getSession(rawSessionID);
    if (!session) return undefined;
    const sessionId = resolveSessionId(rawSessionID);
    if (!sessionId) return undefined;
    return makeBasePayload(sessionId, {{ hook_event_name: "AgentThought", cwd: session.cwd, thought: properties.text }});
  }}
  if (type === "session.next.retried" && stableString(properties.sessionID)) {{
    const rawSessionID = stableString(properties.sessionID);
    const session = getSession(rawSessionID);
    if (!session) return undefined;
    const sessionId = resolveSessionId(rawSessionID);
    if (!sessionId) return undefined;
    const attempt = properties.attempt ?? "?";
    return makeBasePayload(sessionId, {{ hook_event_name: "Notification", cwd: session.cwd, message: `Retrying (attempt ${{attempt}})` }});
  }}
  if (type === "todo.updated" && stableString(properties.sessionID) && Array.isArray(properties.todos)) {{
    const rawSessionID = stableString(properties.sessionID);
    const session = getSession(rawSessionID);
    if (!session) return undefined;
    const sessionId = resolveSessionId(rawSessionID);
    if (!sessionId) return undefined;
    return makeBasePayload(sessionId, {{ hook_event_name: "Notification", cwd: session.cwd, message: `Todo updated: ${{properties.todos.length}} items` }});
  }}
  return undefined;
}}

async function runBridge(payload, captureResponse = false) {{
  const env = {{ ...(globalThis.process?.env ?? {{}}) }};
  Object.assign(env, BRIDGE_ENV);
  if (isObject(payload?._env)) Object.assign(env, payload._env);
  if (typeof payload?._tty === "string" && payload._tty.length > 0) env.TTY = payload._tty;
  const input = JSON.stringify(payload);
  try {{
    if (globalThis.Bun?.spawn) {{
      const subprocess = Bun.spawn(BRIDGE_ARGS, {{ stdin: new Response(input), stdout: captureResponse ? "pipe" : "ignore", stderr: "pipe", env }});
      const exitCode = await subprocess.exited;
      const stderr = subprocess.stderr ? (await new Response(subprocess.stderr).text()).trim() : "";
      if (exitCode !== 0) {{
        if (stderr) console.warn(`[AgentBro] bridge exited ${{exitCode}}: ${{stderr}}`);
        return null;
      }}
      if (!captureResponse || !subprocess.stdout) return null;
      const stdout = (await new Response(subprocess.stdout).text()).trim();
      if (!stdout) return null;
      try {{ return JSON.parse(stdout); }} catch {{ return null; }}
    }}
  }} catch (error) {{
    console.warn(`[AgentBro] Bun bridge spawn failed: ${{error?.message ?? error}}`);
  }}
  try {{
    const childProcess = await import("node:child_process");
    const {{ Buffer }} = await import("node:buffer");
    const stdoutChunks = [];
    const stderrChunks = [];
    const subprocess = childProcess.spawn(BRIDGE_ARGS[0], BRIDGE_ARGS.slice(1), {{
      env,
      windowsHide: true,
      stdio: ["pipe", captureResponse ? "pipe" : "ignore", "pipe"]
    }});
    if (captureResponse && subprocess.stdout) {{
      subprocess.stdout.on("data", (chunk) => stdoutChunks.push(chunk));
    }}
    if (subprocess.stderr) {{
      subprocess.stderr.on("data", (chunk) => stderrChunks.push(chunk));
    }}
    subprocess.stdin.end(input);
    const exitCode = await new Promise((resolve) => {{
      subprocess.on("error", (error) => {{
        console.warn(`[AgentBro] node bridge spawn failed: ${{error?.message ?? error}}`);
        resolve(-1);
      }});
      subprocess.on("close", resolve);
    }});
    if (exitCode !== 0) {{
      const stderr = Buffer.concat(stderrChunks).toString("utf8").trim();
      if (stderr) console.warn(`[AgentBro] node bridge exited ${{exitCode}}: ${{stderr}}`);
      return null;
    }}
    if (!captureResponse) return null;
    const stdout = Buffer.concat(stdoutChunks).toString("utf8").trim();
    if (!stdout) return null;
    try {{ return JSON.parse(stdout); }} catch {{ return null; }}
  }} catch (error) {{
    console.warn(`[AgentBro] bridge fallback failed: ${{error?.message ?? error}}`);
    return null;
  }}
}}

export const server = async ({{ client, serverUrl }}) => {{
  const internalFetch = client?._client?.getConfig?.()?.fetch ?? globalThis.fetch ?? null;
  const serverPort = Number.parseInt(serverUrl?.port || "", 10) || 4096;
  return {{
    name: "AgentBro",
    description: "Forward OpenCode hook events to AgentBro.",
    dispose: async () => {{
      for (const [rawSessionID, session] of sessions.entries()) {{
        if (session._started) {{
          const sessionId = session.parentID ? `opencode-${{session.parentID}}` : `opencode-${{rawSessionID}}`;
          const eventName = session.parentID ? "SubagentStop" : "SessionEnd";
          const payload = makeBasePayload(sessionId, {{
            hook_event_name: eventName,
            cwd: session.cwd,
            agent_name: session.parentID ? `subagent-${{rawSessionID}}` : undefined,
            last_assistant_message: session.lastAssistantText || undefined,
            session_title: session.pendingTitle || undefined
          }});
          await runBridge(payload, false);
        }}
      }}
      sessions.clear();
    }},
    event: async ({{ event }}) => {{
      const payload = mapEvent(event);
      if (!payload) return;
      if (!ENABLED_EVENTS.has(payload.hook_event_name)) return;
      const requestId = stableString(payload._opencode_request_id);
      if (payload.hook_event_name === "PermissionRequest" && payload.tool_name === "AskUserQuestion" && requestId && internalFetch) {{
        const response = await runBridge(payload, true);
        const answers = response?.hookSpecificOutput?.decision?.updatedInput?.answers ?? response?.hookSpecificOutput?.updatedInput?.answers;
        const answerArray = questionAnswerArrays(payload.tool_input?.questions ?? [], answers);
        if (answerArray.length === 0) return;
        await internalFetch(new Request(`http://localhost:${{serverPort}}/question/${{requestId}}/reply`, {{ method: "POST", headers: {{ "Content-Type": "application/json" }}, body: JSON.stringify({{ answers: answerArray }}) }})).catch(() => {{}});
        return;
      }}
      if (payload.hook_event_name === "PermissionRequest" && requestId && internalFetch) {{
        const response = await runBridge(payload, true);
        const behavior = stableString(response?.hookSpecificOutput?.decision?.behavior) ?? stableString(response?.hookSpecificOutput?.permissionDecision);
        if (!behavior) return;
        const reply = behavior === "allow" ? "once" : behavior === "allow_for_session" || behavior === "always" ? "always" : "reject";
        await internalFetch(new Request(`http://localhost:${{serverPort}}/permission/${{requestId}}/reply`, {{ method: "POST", headers: {{ "Content-Type": "application/json" }}, body: JSON.stringify({{ reply }}) }})).catch(() => {{}});
        return;
      }}
      await runBridge(payload, false);
    }},
    "shell.env": async (_input, output) => {{
      for (const key of ENV_KEYS) {{
        const value = process.env[key];
        if (typeof value === "string" && value.length > 0) output.env[key] = value;
      }}
      if (detectedTTY) output.env.TTY = detectedTTY;
    }}
  }};
}};

export default server;
"#
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_cleanup_removes_only_agentbro_hooks() {
        let input = r#"[providers]
name = "keep"

[[hooks]]
event = "SessionStart"
command = "/usr/bin/other"

[[hooks]]
event = "PreToolUse"
command = "/Users/me/.agentbro/bin/agentbro-bridge --source kimi"

[models.default]
name = "also keep"
"#;

        let segments = super::super::toml_hooks::parse_segments(input);
        let cleaned = super::super::toml_hooks::rebuild(&segments, &[], MARKER_PREFIX);
        assert!(cleaned.contains("/usr/bin/other"));
        assert!(cleaned.contains("[providers]"));
        assert!(cleaned.contains("[models.default]"));
        assert!(!cleaned.contains("agentbro-bridge"));
    }

    #[test]
    fn remove_toml_hooks_cleans_legacy_vibe_island_hooks() {
        let tmp = std::env::temp_dir().join(format!(
            "agentbro-kimi-remove-legacy-vibe-{}.toml",
            std::process::id()
        ));
        let original = r#"default_model = "x"

# --- vibe-island Kimi hooks START (managed, do not edit) ---
[[hooks]]
event = "UserPromptSubmit"
command = "/Users/me/.vibe-island/bin/vibe-island-bridge --source kimi"
timeout = 30
# --- vibe-island Kimi hooks END ---

[[hooks]]
event = "PostToolUse"
command = "/usr/local/bin/user-hook.sh"
matcher = "Shell"
"#;
        std::fs::write(&tmp, original).expect("write fixture");

        remove_toml_hooks(&tmp).expect("remove toml hooks");

        let after = std::fs::read_to_string(&tmp).expect("read after");
        assert!(after.contains("default_model"));
        assert!(after.contains("/usr/local/bin/user-hook.sh"));
        assert!(!after.contains("vibe-island"));
        assert!(!after.contains(".vibe-island"));
        assert!(!after.contains("vibe-island-bridge"));

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn legacy_sentinel_cleanup_preserves_surrounding_toml() {
        let input = r#"[providers]
name = "keep"
# [AGENTBRO-START]
[[hooks]]
event = "pre_tool_use"
command = "agentbro"
# [AGENTBRO-END]
[models.default]
name = "also keep"
"#;

        let cleaned = strip_legacy_sentinel_block(input);
        assert!(cleaned.contains("[providers]"));
        assert!(cleaned.contains("[models.default]"));
        assert!(!cleaned.contains("pre_tool_use"));
    }

    #[test]
    fn json_cleanup_preserves_other_hooks() {
        let mut settings = serde_json::json!({
            "hooks": {
                "Notification": [
                    {
                        "matcher": "*",
                        "hooks": [
                            {
                                "type": "command",
                                "command": "/usr/bin/env AGENTBRO_CONFIG_ROOT='/Users/me/.claude' '/Users/me/.agentbro/bin/agentbro-bridge'"
                            }
                        ]
                    },
                    {
                        "matcher": "*",
                        "hooks": [
                            {
                                "type": "command",
                                "command": "/usr/local/bin/custom-notifier --source claude"
                            }
                        ]
                    },
                    {
                        "matcher": "*",
                        "hooks": [
                            {
                                "type": "command",
                                "command": "/Users/me/bin/personal-hook"
                            }
                        ]
                    }
                ]
            }
        });

        strip_json_hooks(&mut settings);

        let entries = settings["hooks"]["Notification"].as_array().unwrap();
        let commands: Vec<&str> = entries
            .iter()
            .flat_map(|entry| entry["hooks"].as_array().unwrap())
            .filter_map(|hook| hook["command"].as_str())
            .collect();

        assert_eq!(commands.len(), 2);
        assert!(commands
            .iter()
            .any(|command| command.contains("custom-notifier")));
        assert!(commands
            .iter()
            .any(|command| command.contains("personal-hook")));
        assert!(!commands
            .iter()
            .any(|command| command.contains("agentbro-bridge")));
    }

    #[test]
    fn collect_commands_handles_top_level_event_array() {
        // Regression: json_event_has_current_profile_command passes the event's
        // array (hooks["SessionStart"]) straight in. collect must recurse into a
        // top-level array, not just objects, or every per-event check sees zero
        // commands and the profile is wrongly reported as needing reinstall.
        let entries = serde_json::json!([
            {
                "matcher": "*",
                "hooks": [
                    {
                        "type": "command",
                        "command": "/usr/bin/env AGENTBRO_HOOK_PORT=17894 /Users/me/.agentbro/bin/agentbro-bridge --source claude-code"
                    }
                ]
            }
        ]);

        let mut commands = Vec::new();
        collect_json_agentbro_commands(&entries, &mut commands);

        assert_eq!(commands.len(), 1);
        assert!(commands[0].contains("agentbro-bridge"));
    }

    #[test]
    fn source_matching_does_not_match_prefixes() {
        let command = "/usr/bin/env AGENTBRO_HOOK_PORT=17894 /Users/me/.agentbro/bin/agentbro-bridge --source qoder-cli";

        assert!(command_source_matches(command, "qoder-cli"));
        assert!(!command_source_matches(command, "qoder"));
        assert!(command_source_matches(
            "/Users/me/.agentbro/bin/agentbro-bridge --source=qoder",
            "qoder"
        ));
    }

    #[test]
    fn profile_json_cleanup_preserves_other_agentbro_sources() {
        let mut settings = serde_json::json!({
            "hooks": {
                "session_start": [
                    {
                        "command": "/usr/bin/env AGENTBRO_HOOK_PORT=17894 /Users/me/.agentbro/bin/agentbro-bridge --source qoder"
                    },
                    {
                        "type": "command",
                        "command": "/usr/bin/env AGENTBRO_HOOK_PORT=17894 /Users/me/.agentbro/bin/agentbro-bridge --source qoder-cli"
                    }
                ]
            }
        });

        strip_json_hooks_for_profile(&mut settings, &qoder_profile());

        let entries = settings["hooks"]["session_start"].as_array().unwrap();
        let commands: Vec<&str> = entries
            .iter()
            .filter_map(|entry| entry["command"].as_str())
            .collect();

        assert_eq!(commands.len(), 1);
        assert!(commands[0].contains("--source qoder-cli"));
    }

    #[test]
    fn qoder_profile_writes_nested_official_hooks() {
        let settings = serde_json::json!({
            "hooks": {
                "SessionStart": [
                    {
                        "hooks": [
                            {
                                "type": "command",
                                "command": "/usr/bin/env AGENTBRO_HOOK_PORT=17894 /Users/me/.agentbro/bin/agentbro-bridge --source qoder-cli"
                            }
                        ]
                    },
                    {
                        "command": "/usr/bin/env AGENTBRO_HOOK_PORT=17894 /Users/me/.agentbro/bin/agentbro-bridge --source qoder"
                    }
                ],
                "session_start": [
                    {
                        "command": "/usr/bin/env AGENTBRO_HOOK_PORT=17894 /Users/me/.agentbro/bin/agentbro-bridge --source qoder"
                    }
                ]
            }
        });
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "agentbro-qoder-profile-{}-{suffix}.json",
            std::process::id()
        ));
        std::fs::write(&path, serde_json::to_string(&settings).unwrap()).unwrap();

        let updated = update_nested_json_hooks(
            &qoder_profile(),
            &path,
            "/usr/bin/env AGENTBRO_HOOK_PORT=17894 /Users/me/.agentbro/bin/agentbro-bridge --source qoder",
        )
        .unwrap();

        let session_start = updated["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(session_start.len(), 2);
        assert!(session_start[0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains("--source qoder-cli"));
        assert!(session_start[1]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains("--source qoder"));
        assert_eq!(
            updated["hooks"]["PreToolUse"][0]["matcher"],
            serde_json::json!("*")
        );
        assert!(updated["hooks"].get("session_start").is_none());

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn json_health_ignores_agentbro_hooks_for_other_sources() {
        let settings = serde_json::json!({
            "hooks": {
                "SessionStart": [
                    {
                        "type": "command",
                        "command": "/usr/bin/env AGENTBRO_HOOK_PORT=17894 /Users/me/.agentbro/bin/agentbro-bridge --source qoder-cli"
                    }
                ]
            }
        });
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "agentbro-json-health-{}-{suffix}.json",
            std::process::id()
        ));
        std::fs::write(&path, serde_json::to_string(&settings).unwrap()).unwrap();

        assert_eq!(
            install_health(&qoder_profile(), &path),
            HookInstallHealth::NotInstalled
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn executable_hook_files_remove_and_report_disabled_managed_files() {
        const EVENTS: &[HookEventDescriptor] = &[plain_event("Enabled"), plain_event("Disabled")];
        let profile = AgentIntegrationProfile {
            id: "test-executable-disabled",
            installation_kind: InstallationKind::ExecutableHookFiles,
            configuration_path: "",
            activation_path: None,
            source: "test-source",
            extra_args: &[],
            events: EVENTS,
        };
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "agentbro-executable-disabled-{}-{suffix}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Disabled"), format!("# {}\n", marker(&profile))).unwrap();
        let effective = BTreeSet::from(["Enabled"]);

        assert!(has_disabled_managed_executable_hook_files(
            &profile, &dir, &effective
        ));
        remove_disabled_executable_hook_files(&profile, &dir, &effective).unwrap();

        assert!(!dir.join("Disabled").exists());
        assert!(!has_disabled_managed_executable_hook_files(
            &profile, &dir, &effective
        ));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rendered_kimi_hooks_use_official_event_names() {
        let profile = kimi_profile();
        let managed = build_managed_hook_entries(&profile, "/tmp/agentbro-bridge --source kimi");
        let rendered = super::super::toml_hooks::rebuild(&[], &managed, &marker(&profile));

        // All 13 documented Kimi events present.
        for event in &[
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "PostToolUseFailure",
            "Notification",
            "Stop",
            "StopFailure",
            "SessionStart",
            "SessionEnd",
            "SubagentStart",
            "SubagentStop",
            "PreCompact",
            "PostCompact",
        ] {
            assert!(
                rendered.contains(&format!("event = \"{}\"", event)),
                "missing event {} in rendered output:\n{}",
                event,
                rendered
            );
        }

        // PermissionRequest was a phantom event we used to emit; remove safety.
        assert!(
            !rendered.contains("PermissionRequest"),
            "PermissionRequest is not a real Kimi event, must not appear"
        );

        // matcher must be empty string (regex "match all"), never "*" (invalid regex).
        assert!(
            !rendered.contains("matcher = \"*\""),
            "matcher = \"*\" is invalid regex per Kimi spec"
        );
        assert!(rendered.contains("matcher = \"\""));

        // No bogus huge timeout left over from the PermissionRequest hook.
        assert!(!rendered.contains("timeout = 86400"));

        assert!(rendered.contains(MARKER_PREFIX));
    }

    #[test]
    fn rendered_deepseek_hooks_use_deepseek_source() {
        let profile = deepseek_profile();
        let managed =
            build_managed_hook_entries(&profile, "/tmp/agentbro-bridge --source deepseek");
        let rendered = super::super::toml_hooks::rebuild(&[], &managed, &marker(&profile));
        assert!(rendered.contains("event = \"SessionStart\""));
        assert!(rendered.contains("event = \"SubagentStart\""));
        assert!(rendered.contains("--source deepseek"));
        assert!(rendered.contains(MARKER_PREFIX));
    }

    #[test]
    fn update_toml_hooks_rejects_conflicting_hooks_table_without_modifying_file() {
        let tmp = std::env::temp_dir().join(format!(
            "agentbro-kimi-conflict-{}.toml",
            std::process::id()
        ));
        let original = "default_model = \"x\"\n[hooks]\nfoo = \"bar\"\n";
        std::fs::write(&tmp, original).expect("write fixture");

        let profile = kimi_profile();
        let result = update_toml_hooks(&profile, &tmp);
        assert!(result.is_err(), "expected conflict error");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("[hooks]"));
        assert!(err.contains(&tmp.display().to_string()));
        assert!(
            err.contains(":2"),
            "error should report line number 2: {}",
            err
        );

        let after = std::fs::read_to_string(&tmp).expect("read after");
        assert_eq!(after, original, "file must be untouched on conflict");

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn update_toml_hooks_is_idempotent() {
        let tmp = std::env::temp_dir().join(format!(
            "agentbro-kimi-idempotent-{}.toml",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&tmp);
        std::fs::write(&tmp, "default_model = \"x\"\n").expect("write fixture");

        let profile = kimi_profile();
        update_toml_hooks(&profile, &tmp).expect("first install");
        let after_first = std::fs::read_to_string(&tmp).expect("read after first");

        // Second install with identical state should produce identical bytes.
        update_toml_hooks(&profile, &tmp).expect("second install");
        let after_second = std::fs::read_to_string(&tmp).expect("read after second");
        assert_eq!(after_first, after_second);

        // Marker comment appears exactly once across reinstalls.
        assert_eq!(after_second.matches(MARKER_PREFIX).count(), 1);

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn registered_adapters_expose_only_their_profile_events() {
        let adapters = crate::agents::all_adapters();
        let missing_profiles: Vec<String> = adapters
            .iter()
            .filter(|adapter| profile_for_agent(adapter.name()).is_none())
            .map(|adapter| adapter.name().to_string())
            .collect();
        assert!(
            missing_profiles.is_empty(),
            "missing hook profiles for adapters: {}",
            missing_profiles.join(", ")
        );

        for adapter in adapters {
            let profile = profile_for_agent(adapter.name()).unwrap();
            let supported_events: Vec<&str> =
                profile.events.iter().map(|event| event.name).collect();
            let statuses = event_statuses(&profile);
            let displayed_events: Vec<&str> =
                statuses.iter().map(|event| event.name.as_str()).collect();

            assert_eq!(
                displayed_events,
                supported_events,
                "hook events for {} must come from its own profile",
                adapter.name()
            );
        }
    }
}
