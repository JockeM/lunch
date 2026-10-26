mod slack;
mod text;

use crate::domain::{FailureStage, LunchState, RestaurantId, RestaurantLunch, SourceError};

pub use slack::render_slack_payload;
pub use text::render_day;

fn order_link(lunch: &RestaurantLunch) -> Option<(&'static str, &'static str)> {
    match (lunch.meta.id, &lunch.state) {
        (RestaurantId::JinxEmpire, LunchState::Available { .. }) => Some((
            "https://order.openpos.cloud/jinxempire/019a76e1-5979-7d97-b49a-4a9dd57dac33?label=666",
            "Order online — 10 kr off",
        )),
        _ => None,
    }
}

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
        SourceError::OutputTooLarge {
            field,
            actual,
            limit,
        } => {
            format!("{field} too large: {actual} exceeds limit {limit}")
        }
        SourceError::MissingStructuredData => "missing structured data".to_string(),
        SourceError::MissingExpectedElement(element) => {
            format!("missing expected element {element}")
        }
        SourceError::InvalidJson(message) => format!("invalid JSON: {message}"),
        SourceError::UnsupportedFormat(message) => format!("unsupported format: {message}"),
    }
}
