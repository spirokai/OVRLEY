use ovrley_core::CoreError;
use std::path::PathBuf;

#[test]
fn config_error_display() {
    let err = CoreError::Config("missing field: fps".into());
    assert_eq!(err.to_string(), "Invalid configuration: missing field: fps");
}

#[test]
fn activity_error_display() {
    let err = CoreError::Activity("invalid timestamp".into());
    assert_eq!(err.to_string(), "Activity parse error: invalid timestamp");
}

#[test]
fn render_error_display() {
    let err = CoreError::Render("surface creation failed".into());
    assert_eq!(err.to_string(), "Render error: surface creation failed");
}

#[test]
fn encode_error_display() {
    let err = CoreError::Encode("pipeline setup failed".into());
    assert_eq!(err.to_string(), "Encoding error: pipeline setup failed");
}

#[test]
fn io_error_display() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
    let err = CoreError::Io {
        path: PathBuf::from("/tmp/test.mp4"),
        source: io_err,
    };
    let msg = err.to_string();
    assert!(
        msg.contains("/tmp/test.mp4"),
        "IO error should include path"
    );
    assert!(
        msg.contains("file not found"),
        "IO error should include source"
    );
}

#[test]
fn ffmpeg_error_display() {
    let _err = CoreError::FfmpegNotFound("ffmpeg binary not in PATH".into());
}

#[test]
fn cancelled_error_display() {
    let err = CoreError::Cancelled;
    assert!(err.to_string().to_lowercase().contains("cancelled"));
}

#[test]
fn serialization_error_from() {
    let json_err = serde_json::from_str::<serde_json::Value>("not valid json").unwrap_err();
    let core_err: CoreError = json_err.into();
    let msg = core_err.to_string();
    assert!(
        !msg.is_empty(),
        "Serialization error message should not be empty"
    );
    assert!(
        msg.to_lowercase().contains("json")
            || msg.to_lowercase().contains("expected")
            || msg.to_lowercase().contains("invalid"),
        "Serialization error should indicate a parse problem, got: '{msg}'"
    );
}
