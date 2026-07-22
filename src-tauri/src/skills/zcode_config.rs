use serde_json::{Map, Value};
use std::collections::HashSet;

pub fn mcp_servers(config: &Value) -> Option<&Map<String, Value>> {
    config.pointer("/mcp/servers").and_then(Value::as_object)
}

pub fn mcp_servers_mut(config: &mut Value) -> Result<&mut Map<String, Value>, String> {
    nested_object_mut(config, "mcp", "servers")
}

pub fn enabled_plugins(config: &Value) -> Option<&Map<String, Value>> {
    config
        .pointer("/plugins/enabledPlugins")
        .and_then(Value::as_object)
}

pub fn enabled_plugins_mut(config: &mut Value) -> Result<&mut Map<String, Value>, String> {
    nested_object_mut(config, "plugins", "enabledPlugins")
}

pub fn disabled_skill_paths(config: &Value) -> HashSet<String> {
    config
        .get("skill")
        .and_then(Value::as_object)
        .map(|skills| {
            skills
                .iter()
                .filter(|(_, value)| value.get("enable").and_then(Value::as_bool) == Some(false))
                .map(|(path, _)| path.clone())
                .collect()
        })
        .unwrap_or_default()
}

pub fn skill_overrides_mut(config: &mut Value) -> Result<&mut Map<String, Value>, String> {
    root_object_mut(config)?
        .entry("skill")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| "ZCode skill config is not an object".to_string())
}

fn nested_object_mut<'a>(
    config: &'a mut Value,
    parent: &str,
    child: &str,
) -> Result<&'a mut Map<String, Value>, String> {
    let parent = root_object_mut(config)?
        .entry(parent)
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| format!("ZCode {parent} config is not an object"))?;
    parent
        .entry(child)
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| format!("ZCode {child} config is not an object"))
}

fn root_object_mut(config: &mut Value) -> Result<&mut Map<String, Value>, String> {
    config
        .as_object_mut()
        .ok_or_else(|| "ZCode config is not an object".to_string())
}
