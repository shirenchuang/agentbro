// Display detection and multi-monitor support

use serde::Serialize;

/// Information about a connected display, sent to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayInfo {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
    pub is_primary: bool,
}

/// List all connected displays.
pub fn list_displays_inner(app: &tauri::AppHandle) -> Vec<DisplayInfo> {
    let monitors = match app.available_monitors() {
        Ok(m) => m,
        Err(_) => return vec![],
    };

    let primary = app.primary_monitor().ok().flatten();
    let primary_name = primary
        .as_ref()
        .and_then(|p| p.name())
        .map(|s| s.to_string());

    monitors
        .into_iter()
        .map(|m| {
            let name = m.name().map(|s| s.to_string()).unwrap_or_default();
            let size = m.size();
            let is_primary = primary_name.as_deref() == Some(name.as_str());
            DisplayInfo {
                name,
                width: size.width,
                height: size.height,
                scale_factor: m.scale_factor(),
                is_primary,
            }
        })
        .collect()
}

/// Find the monitor matching `display_id` ("primary" or a monitor name).
/// Falls back to primary if no match found.
pub fn find_target_monitor(
    app: &tauri::AppHandle,
    display_id: &str,
) -> Option<tauri::Monitor> {
    if display_id == "primary" || display_id == "auto" || display_id.is_empty() {
        return app.primary_monitor().ok().flatten();
    }

    let monitors = app.available_monitors().ok()?;
    monitors
        .into_iter()
        .find(|m| m.name().map(|s| s.as_str()) == Some(display_id))
        .or_else(|| app.primary_monitor().ok().flatten())
}
