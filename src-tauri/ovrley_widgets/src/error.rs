#[derive(Debug, thiserror::Error)]
pub enum WidgetError {
    #[error("Invalid configuration: {0}")]
    Config(String),
    #[error("Widget rendering failed: {0}")]
    Render(String),
}

pub type WidgetResult<T> = Result<T, WidgetError>;
