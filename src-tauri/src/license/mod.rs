// License System — Device fingerprint, trial tracking, license validation
// MVP: local trial + offline grace, activation placeholder for future server

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tauri::{AppHandle, Emitter};

/// Trial duration in days
const TRIAL_DAYS: i64 = 14;

/// Offline grace period in days
const OFFLINE_GRACE_DAYS: i64 = 7;

/// License status reported to the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LicenseStatus {
    /// License is active and valid
    Active {
        license_key: String,
        device_id: String,
    },
    /// In trial period
    Trial {
        days_remaining: i64,
        device_id: String,
    },
    /// Trial has expired
    TrialExpired { device_id: String },
    /// License key is invalid
    Invalid { reason: String },
    /// Offline grace period — license was valid but can't reach server
    OfflineGrace {
        days_remaining: i64,
        license_key: String,
    },
}

/// Persisted license data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseData {
    /// First launch timestamp (Unix seconds)
    pub first_launch: i64,
    /// Active license key (if any)
    pub license_key: Option<String>,
    /// When the license was last validated online (Unix seconds)
    pub last_validated: Option<i64>,
    /// Device fingerprint
    pub device_id: String,
}

/// License manager
pub struct LicenseManager {
    data: Arc<RwLock<LicenseData>>,
    data_path: PathBuf,
    app_handle: Option<AppHandle>,
}

impl LicenseManager {
    /// Create a new LicenseManager, loading persisted state
    pub fn new() -> Self {
        let data_path = Self::license_file_path();
        let device_id = Self::get_device_fingerprint();

        let data = Self::load_from_disk(&data_path).unwrap_or_else(|| {
            let now = chrono::Utc::now().timestamp();
            LicenseData {
                first_launch: now,
                license_key: None,
                last_validated: None,
                device_id: device_id.clone(),
            }
        });

        let manager = Self {
            data: Arc::new(RwLock::new(data.clone())),
            data_path,
            app_handle: None,
        };

        // Persist on first creation
        let _ = manager.save_to_disk();

        manager
    }

    /// Set the Tauri app handle for emitting events
    pub fn set_app_handle(&mut self, handle: AppHandle) {
        self.app_handle = Some(handle);
    }

    /// Get current license status
    pub fn check(&self) -> LicenseStatus {
        let data = match self.data.read() {
            Ok(d) => d.clone(),
            Err(_) => {
                return LicenseStatus::Invalid {
                    reason: "Lock error".to_string(),
                }
            }
        };

        let now = chrono::Utc::now().timestamp();

        // If there's an active license key
        if let Some(ref key) = data.license_key {
            // Check if we've validated recently (within offline grace period)
            if let Some(last_validated) = data.last_validated {
                let days_since = (now - last_validated) / 86400;
                if days_since <= OFFLINE_GRACE_DAYS {
                    return LicenseStatus::Active {
                        license_key: key.clone(),
                        device_id: data.device_id.clone(),
                    };
                } else {
                    // Offline grace period
                    let grace_remaining = OFFLINE_GRACE_DAYS - days_since;
                    if grace_remaining > 0 {
                        return LicenseStatus::OfflineGrace {
                            days_remaining: grace_remaining,
                            license_key: key.clone(),
                        };
                    }
                }
            }

            // Key exists but never validated or grace expired
            // For MVP, accept the key as valid (no server validation yet)
            return LicenseStatus::Active {
                license_key: key.clone(),
                device_id: data.device_id.clone(),
            };
        }

        // No license key — check trial
        let days_elapsed = (now - data.first_launch) / 86400;
        let days_remaining = TRIAL_DAYS - days_elapsed;

        if days_remaining > 0 {
            LicenseStatus::Trial {
                days_remaining,
                device_id: data.device_id.clone(),
            }
        } else {
            LicenseStatus::TrialExpired {
                device_id: data.device_id.clone(),
            }
        }
    }

    /// Activate a license key
    pub fn activate(&self, license_key: &str) -> Result<LicenseStatus, String> {
        if license_key.trim().is_empty() {
            return Err("License key cannot be empty".to_string());
        }

        // MVP: accept any non-empty key (future: validate with server)
        // Basic format check: expect something like XXXX-XXXX-XXXX-XXXX
        let key = license_key.trim().to_uppercase();

        let now = chrono::Utc::now().timestamp();

        {
            let mut data = self
                .data
                .write()
                .map_err(|e| format!("Lock error: {}", e))?;
            data.license_key = Some(key.clone());
            data.last_validated = Some(now);
        }

        self.save_to_disk()
            .map_err(|e| format!("Failed to save: {}", e))?;
        self.emit_status_change();

        let device_id = self.get_device_id();
        Ok(LicenseStatus::Active {
            license_key: key,
            device_id,
        })
    }

    /// Deactivate the current license
    pub fn deactivate(&self) -> Result<LicenseStatus, String> {
        {
            let mut data = self
                .data
                .write()
                .map_err(|e| format!("Lock error: {}", e))?;
            data.license_key = None;
            data.last_validated = None;
        }

        self.save_to_disk()
            .map_err(|e| format!("Failed to save: {}", e))?;
        self.emit_status_change();

        Ok(self.check())
    }

    /// Get the device ID
    pub fn get_device_id(&self) -> String {
        self.data
            .read()
            .map(|d| d.device_id.clone())
            .unwrap_or_else(|_| "unknown".to_string())
    }

    // ── Device Fingerprint ────────────────────────────────────────

    /// Get a stable device fingerprint
    /// macOS: IOPlatformUUID from IOKit
    /// Fallback: generate and persist a UUID
    fn get_device_fingerprint() -> String {
        // Try macOS IOPlatformUUID
        if let Some(uuid) = Self::get_macos_hardware_uuid() {
            return uuid;
        }

        // Fallback: check for persisted fingerprint or generate new one
        let path = Self::license_file_path();
        if let Some(data) = Self::load_from_disk(&path) {
            if !data.device_id.is_empty() {
                return data.device_id;
            }
        }

        // Generate new UUID as last resort
        uuid::Uuid::new_v4().to_string()
    }

    /// Get macOS hardware UUID via ioreg
    #[cfg(target_os = "macos")]
    fn get_macos_hardware_uuid() -> Option<String> {
        let output = std::process::Command::new("ioreg")
            .args(["-rd1", "-c", "IOPlatformExpertDevice"])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.contains("IOPlatformUUID") {
                // Line format: "IOPlatformUUID" = "XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX"
                if let Some(uuid_start) = trimmed.rfind('"') {
                    let before = &trimmed[..uuid_start];
                    if let Some(uuid_begin) = before.rfind('"') {
                        let uuid = &before[uuid_begin + 1..];
                        if uuid.len() >= 32 {
                            return Some(uuid.to_string());
                        }
                    }
                }
            }
        }

        None
    }

    #[cfg(not(target_os = "macos"))]
    fn get_macos_hardware_uuid() -> Option<String> {
        None
    }

    // ── Persistence ───────────────────────────────────────────────

    fn license_file_path() -> PathBuf {
        let base = dirs::config_dir()
            .or_else(|| dirs::data_local_dir())
            .unwrap_or_else(std::env::temp_dir);
        base.join("agentbro").join("license.json")
    }

    fn load_from_disk(path: &PathBuf) -> Option<LicenseData> {
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    fn save_to_disk(&self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = self.data_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = self.data.read().map_err(|e| format!("Lock error: {}", e))?;
        let content = serde_json::to_string_pretty(&*data)?;
        std::fs::write(&self.data_path, content)?;
        Ok(())
    }

    fn emit_status_change(&self) {
        if let Some(ref handle) = self.app_handle {
            let status = self.check();
            if let Err(e) = handle.emit("license-status-changed", &status) {
                log::error!("Failed to emit license-status-changed: {}", e);
            }
        }
    }
}
