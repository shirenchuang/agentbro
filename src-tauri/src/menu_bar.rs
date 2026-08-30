use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder};
use tauri::{Manager, Wry};

pub const TRAY_ID: &str = "agentbro-tray";
pub const SKILL_PACK_PICKER_ID: &str = "skill-pack-picker";

pub struct MenuBarLabels {
    pub open: &'static str,
    pub skill_packs: &'static str,
    pub settings: &'static str,
    pub quit: &'static str,
}

pub fn labels(language: &str) -> MenuBarLabels {
    match language {
        "zh" => MenuBarLabels {
            open: "打开 AgentBro",
            skill_packs: "技能包…",
            settings: "设置",
            quit: "退出",
        },
        "ja" => MenuBarLabels {
            open: "AgentBro を開く",
            skill_packs: "スキルパック…",
            settings: "設定",
            quit: "終了",
        },
        "ko" => MenuBarLabels {
            open: "AgentBro 열기",
            skill_packs: "스킬 팩…",
            settings: "설정",
            quit: "종료",
        },
        "tr" => MenuBarLabels {
            open: "AgentBro'yu Aç",
            skill_packs: "Beceri Paketleri…",
            settings: "Ayarlar",
            quit: "Çıkış",
        },
        _ => MenuBarLabels {
            open: "Open AgentBro",
            skill_packs: "Skill Packs…",
            settings: "Settings",
            quit: "Quit",
        },
    }
}

pub fn build_tray_menu<M: Manager<Wry>>(manager: &M, language: &str) -> tauri::Result<Menu<Wry>> {
    let labels = labels(language);
    let show_item = MenuItemBuilder::with_id("show", labels.open).build(manager)?;
    let settings_item = MenuItemBuilder::with_id("settings", labels.settings).build(manager)?;
    let quit_item = MenuItemBuilder::with_id("quit", labels.quit).build(manager)?;

    MenuBuilder::new(manager)
        .item(&show_item)
        .item(&settings_item)
        .separator()
        .item(&quit_item)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_labels_follow_the_selected_language() {
        assert_eq!(labels("zh").skill_packs, "技能包…");
        assert_eq!(labels("en").skill_packs, "Skill Packs…");
    }
}
