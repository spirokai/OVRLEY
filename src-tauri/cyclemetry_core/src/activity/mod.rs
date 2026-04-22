use serde_json::Value;

pub fn parse_activity_json(input: &str) -> Result<Value, String> {
    serde_json::from_str(input).map_err(|error| format!("Invalid parsedActivity JSON: {error}"))
}
