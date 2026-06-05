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

#[derive(Debug, Clone)]
struct SystemDisplayLabel {
    product_id: Option<String>,
    label: String,
    width: Option<u32>,
    height: Option<u32>,
}

fn parse_monitor_product_id(name: &str) -> Option<String> {
    name.strip_prefix("Monitor #")
        .and_then(|id| id.trim().parse::<u32>().ok())
        .map(|id| id.to_string())
}

fn is_generic_monitor_name(name: &str) -> bool {
    name.trim().is_empty() || parse_monitor_product_id(name).is_some()
}

#[cfg(target_os = "macos")]
fn parse_resolution(raw: &str) -> Option<(u32, u32)> {
    let numbers: Vec<u32> = raw
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u32>().ok())
        .collect();
    if numbers.len() >= 2 {
        Some((numbers[0], numbers[1]))
    } else {
        None
    }
}

fn size_matches(label: &SystemDisplayLabel, width: u32, height: u32) -> bool {
    let (Some(label_width), Some(label_height)) = (label.width, label.height) else {
        return false;
    };

    (label_width == width && label_height == height)
        || (label_width == height && label_height == width)
        || (label_width == width.saturating_mul(2) && label_height == height.saturating_mul(2))
        || (label_width.saturating_mul(2) == width && label_height.saturating_mul(2) == height)
}

fn consume_system_label(
    labels: &[SystemDisplayLabel],
    used_labels: &mut [bool],
    predicate: impl Fn(&SystemDisplayLabel) -> bool,
) -> Option<String> {
    let index = labels
        .iter()
        .enumerate()
        .find_map(|(index, label)| (!used_labels[index] && predicate(label)).then_some(index))?;
    used_labels[index] = true;
    Some(labels[index].label.clone())
}

fn resolve_display_label(
    id: &str,
    name: &str,
    width: u32,
    height: u32,
    labels: &[SystemDisplayLabel],
    used_labels: &mut [bool],
) -> String {
    if !id.is_empty() {
        if let Some(label) = consume_system_label(labels, used_labels, |label| {
            label.product_id.as_deref() == Some(id)
        }) {
            return label;
        }
    }

    if is_generic_monitor_name(name) {
        if let Some(label) = consume_system_label(labels, used_labels, |label| {
            size_matches(label, width, height)
        }) {
            return label;
        }
        if let Some(label) = consume_system_label(labels, used_labels, |_| true) {
            return label;
        }
    }

    if name.is_empty() {
        "Display".to_string()
    } else {
        name.to_string()
    }
}

#[cfg(target_os = "macos")]
fn system_display_labels() -> Vec<SystemDisplayLabel> {
    let output = match std::process::Command::new("system_profiler")
        .args(["SPDisplaysDataType", "-json"])
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };

    let root: serde_json::Value = match serde_json::from_slice(&output.stdout) {
        Ok(root) => root,
        Err(_) => return Vec::new(),
    };

    let mut labels = Vec::new();
    let Some(gpus) = root.get("SPDisplaysDataType").and_then(|v| v.as_array()) else {
        return labels;
    };

    for gpu in gpus {
        let Some(displays) = gpu.get("spdisplays_ndrvs").and_then(|v| v.as_array()) else {
            continue;
        };
        for display in displays {
            let product_id = display
                .get("_spdisplays_display-product-id")
                .and_then(|v| v.as_str())
                .and_then(|product_hex| u32::from_str_radix(product_hex, 16).ok())
                .map(|id| id.to_string());
            let name = display
                .get("_name")
                .and_then(|v| v.as_str())
                .unwrap_or("Display");
            let resolution_raw = display
                .get("spdisplays_resolution")
                .or_else(|| display.get("_spdisplays_resolution"))
                .and_then(|v| v.as_str());
            let resolution = resolution_raw
                .and_then(|raw| raw.split('@').next())
                .map(|raw| raw.trim().replace(" x ", "x"))
                .filter(|raw| !raw.is_empty());

            let label = resolution
                .map(|resolution| format!("{name} ({resolution})"))
                .unwrap_or_else(|| name.to_string());
            let (width, height) = resolution_raw
                .and_then(parse_resolution)
                .map(|(width, height)| (Some(width), Some(height)))
                .unwrap_or((None, None));
            labels.push(SystemDisplayLabel {
                product_id,
                label,
                width,
                height,
            });
        }
    }

    labels
}

#[cfg(not(target_os = "macos"))]
fn system_display_labels() -> Vec<SystemDisplayLabel> {
    Vec::new()
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
    let mut used_display_labels = vec![false; display_labels.len()];

    monitors
        .into_iter()
        .map(|m| {
            let name = m.name().map(|s| s.to_string()).unwrap_or_default();
            let id = parse_monitor_product_id(&name).unwrap_or_else(|| name.clone());
            let size = m.size();
            let label = resolve_display_label(
                &id,
                &name,
                size.width,
                size.height,
                &display_labels,
                &mut used_display_labels,
            );
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

pub fn find_cursor_monitor(app: &tauri::AppHandle) -> Option<tauri::Monitor> {
    if let Ok(cursor) = app.cursor_position() {
        if let Ok(monitors) = app.available_monitors() {
            if let Some(monitor) = monitors.iter().find(|m| {
                let scale = m.scale_factor().max(1.0);
                let pos = m.position();
                let size = m.size();
                let x = pos.x as f64 / scale;
                let y = pos.y as f64 / scale;
                let width = size.width as f64 / scale;
                let height = size.height as f64 / scale;
                cursor.x >= x && cursor.x <= x + width && cursor.y >= y && cursor.y <= y + height
            }) {
                return Some(monitor.clone());
            }
        }
    }

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
