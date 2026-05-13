// Display detection and multi-monitor support

use serde::Serialize;

/// Information about a connected display, sent to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayInfo {
    pub id: String,
    pub name: String,
    pub label: String,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
    pub is_primary: bool,
}

fn parse_monitor_product_id(name: &str) -> Option<String> {
    name.strip_prefix("Monitor #")
        .and_then(|id| id.trim().parse::<u32>().ok())
        .map(|id| id.to_string())
}

#[cfg(target_os = "macos")]
fn system_display_labels() -> std::collections::HashMap<String, String> {
    let output = match std::process::Command::new("system_profiler")
        .args(["SPDisplaysDataType", "-json"])
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return std::collections::HashMap::new(),
    };

    let root: serde_json::Value = match serde_json::from_slice(&output.stdout) {
        Ok(root) => root,
        Err(_) => return std::collections::HashMap::new(),
    };

    let mut labels = std::collections::HashMap::new();
    let Some(gpus) = root.get("SPDisplaysDataType").and_then(|v| v.as_array()) else {
        return labels;
    };

    for gpu in gpus {
        let Some(displays) = gpu.get("spdisplays_ndrvs").and_then(|v| v.as_array()) else {
            continue;
        };
        for display in displays {
            let Some(product_hex) = display
                .get("_spdisplays_display-product-id")
                .and_then(|v| v.as_str())
            else {
                continue;
            };
            let Ok(product_id) = u32::from_str_radix(product_hex, 16) else {
                continue;
            };
            let name = display
                .get("_name")
                .and_then(|v| v.as_str())
                .unwrap_or("Display");
            let resolution = display
                .get("spdisplays_resolution")
                .or_else(|| display.get("_spdisplays_resolution"))
                .and_then(|v| v.as_str())
                .and_then(|raw| raw.split('@').next())
                .map(|raw| raw.replace(' ', ""))
                .filter(|raw| !raw.is_empty());

            let label = resolution
                .map(|resolution| format!("{name} ({resolution})"))
                .unwrap_or_else(|| name.to_string());
            labels.insert(product_id.to_string(), label);
        }
    }

    labels
}

#[cfg(not(target_os = "macos"))]
fn system_display_labels() -> std::collections::HashMap<String, String> {
    std::collections::HashMap::new()
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
    let display_labels = system_display_labels();

    monitors
        .into_iter()
        .map(|m| {
            let name = m.name().map(|s| s.to_string()).unwrap_or_default();
            let id = parse_monitor_product_id(&name).unwrap_or_else(|| name.clone());
            let label = display_labels
                .get(&id)
                .cloned()
                .unwrap_or_else(|| name.clone());
            let size = m.size();
            let is_primary = primary_name.as_deref() == Some(name.as_str());
            DisplayInfo {
                id,
                name,
                label,
                width: size.width,
                height: size.height,
                scale_factor: m.scale_factor(),
                is_primary,
            }
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn cursor_position() -> Option<(f64, f64)> {
    let output = std::process::Command::new("osascript")
        .args([
            "-e",
            r#"
            use framework "AppKit"
            set mouseLoc to current application's NSEvent's mouseLocation()
            set x to mouseLoc's x as real
            set y to mouseLoc's y as real
            return (x as text) & "," & (y as text)
        "#,
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut parts = stdout.trim().split(',');
    let x = parts.next()?.parse::<f64>().ok()?;
    let y = parts.next()?.parse::<f64>().ok()?;
    Some((x, y))
}

#[cfg(not(target_os = "macos"))]
fn cursor_position() -> Option<(f64, f64)> {
    None
}

fn find_cursor_monitor(app: &tauri::AppHandle) -> Option<tauri::Monitor> {
    let (cursor_x, cursor_y) = cursor_position()?;
    let monitors = app.available_monitors().ok()?;

    let by_rect = monitors.iter().find(|m| {
        let scale = m.scale_factor();
        let pos = m.position();
        let size = m.size();
        let x = pos.x as f64 / scale;
        let y = pos.y as f64 / scale;
        let width = size.width as f64 / scale;
        let height = size.height as f64 / scale;
        cursor_x >= x && cursor_x <= x + width && cursor_y >= y && cursor_y <= y + height
    });

    if let Some(monitor) = by_rect {
        return Some(monitor.clone());
    }

    // AppKit and Tauri can disagree on the vertical origin on macOS. The x-axis
    // fallback still handles common side-by-side monitor layouts, but stacked
    // monitors often share the same x range. In that ambiguous case we should
    // fall back to the primary display instead of picking whichever monitor the
    // OS happens to enumerate first.
    let x_matches: Vec<_> = monitors
        .into_iter()
        .filter(|m| {
            let scale = m.scale_factor();
            let x = m.position().x as f64 / scale;
            let width = m.size().width as f64 / scale;
            cursor_x >= x && cursor_x <= x + width
        })
        .collect();
    if x_matches.len() == 1 {
        x_matches.into_iter().next()
    } else {
        None
    }
}

/// Find the monitor matching `display_id` ("primary", "auto", or a monitor name).
/// Falls back to primary if no match found.
pub fn find_target_monitor(app: &tauri::AppHandle, display_id: &str) -> Option<tauri::Monitor> {
    if display_id == "auto" {
        return find_cursor_monitor(app).or_else(|| app.primary_monitor().ok().flatten());
    }

    if display_id == "primary" || display_id.is_empty() {
        return app.primary_monitor().ok().flatten();
    }

    let monitors = app.available_monitors().ok()?;
    monitors
        .into_iter()
        .find(|m| {
            let name = m.name().map(|s| s.to_string()).unwrap_or_default();
            name == display_id || parse_monitor_product_id(&name).as_deref() == Some(display_id)
        })
        .or_else(|| app.primary_monitor().ok().flatten())
}
