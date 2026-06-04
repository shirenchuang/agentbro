fn main() {
    println!("cargo:rerun-if-env-changed=AGENTBRO_TELEMETRY_SLS_HOST");
    println!("cargo:rerun-if-env-changed=AGENTBRO_TELEMETRY_SLS_PROJECT");
    println!("cargo:rerun-if-env-changed=AGENTBRO_TELEMETRY_SLS_LOGSTORE");
    ensure_bridge_resource_placeholder();
    tauri_build::build()
}

fn ensure_bridge_resource_placeholder() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let resource_dir = manifest_dir.join("target").join("agentbro-bridge-resource");
    let resource_path = resource_dir.join("agentbro-bridge");
    if resource_path.exists() {
        return;
    }
    if std::fs::create_dir_all(&resource_dir).is_ok() {
        let _ = std::fs::write(resource_path, []);
    }
}
