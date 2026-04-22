use serde_json::Value;

pub fn parse_config_json(input: &str) -> Result<Value, String> {
    serde_json::from_str(input).map_err(|error| format!("Invalid config JSON: {error}"))
}
