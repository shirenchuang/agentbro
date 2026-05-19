use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SwitchAppType {
    Claude,
    Codex,
    Gemini,
    OpenCode,
    Hermes,
}

impl SwitchAppType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
            Self::OpenCode => "opencode",
            Self::Hermes => "hermes",
        }
    }

    pub fn from_cc_switch(cc_type: &str) -> Option<Self> {
        match cc_type {
            "claude" | "claude-desktop" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "gemini" => Some(Self::Gemini),
            "opencode" | "openclaw" => Some(Self::OpenCode),
            "hermes" => Some(Self::Hermes),
            _ => None,
        }
    }

    pub fn all() -> &'static [SwitchAppType] {
        &[
            Self::Claude,
            Self::Codex,
            Self::Gemini,
            Self::OpenCode,
            Self::Hermes,
        ]
    }
}

impl std::fmt::Display for SwitchAppType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for SwitchAppType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "claude" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
            "gemini" => Ok(Self::Gemini),
            "opencode" => Ok(Self::OpenCode),
            "hermes" => Ok(Self::Hermes),
            other => Err(format!("unknown app type: {other}")),
        }
    }
}
