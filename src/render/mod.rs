mod slack;
mod text;

use crate::domain::{FailureStage, SourceError};

pub use slack::render_slack_payload;
pub use text::render_day;

fn render_no_lunch_reason(reason: &crate::domain::NoLunchReason) -> &'static str {
    match reason {
        crate::domain::NoLunchReason::Weekend => "weekend",
        crate::domain::NoLunchReason::Closed => "closed",
        crate::domain::NoLunchReason::MissingDay => "missing day in menu",
        crate::domain::NoLunchReason::EmptyMenu => "empty menu",
    }
}

fn render_stage(stage: FailureStage) -> &'static str {
    match stage {
        FailureStage::Fetch => "fetch",
        FailureStage::Parse => "parse",
        FailureStage::Normalize => "normalize",
    }
}

fn render_error(error: &SourceError) -> String {
    match error {
        SourceError::NotImplemented => "parser not implemented yet".to_string(),
        SourceError::Network(message) => format!("network failure: {message}"),
        SourceError::HttpStatus(status) => format!("HTTP status {status}"),
        SourceError::MissingStructuredData => "missing structured data".to_string(),
        SourceError::MissingExpectedElement(element) => {
            format!("missing expected element {element}")
        }
        SourceError::InvalidJson(message) => format!("invalid JSON: {message}"),
        SourceError::UnsupportedFormat(message) => format!("unsupported format: {message}"),
    }
}
