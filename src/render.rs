use crate::date::Weekday;
use crate::domain::{
    FailureStage, LunchItem, LunchState, NoLunchReason, RestaurantLunch, SourceError,
};

pub fn render_day(weekday: Weekday, lunches: &[RestaurantLunch]) -> String {
    let mut output = format!("Today ({weekday})");

    for lunch in lunches {
        output.push_str("\n\n");
        output.push_str(lunch.meta.display_name);
        output.push('\n');
        output.push_str(&render_state(&lunch.state));
    }

    output
}

fn render_state(state: &LunchState) -> String {
    match state {
        LunchState::Available { items, .. } => render_available(items),
        LunchState::NoLunchToday {
            weekday, reason, ..
        } => {
            format!("No lunch for {weekday}: {}", render_no_lunch_reason(reason))
        }
        LunchState::Unavailable { stage, error } => {
            format!(
                "Unavailable: {} error, {}",
                render_stage(*stage),
                render_error(error)
            )
        }
    }
}

fn render_available(items: &[LunchItem]) -> String {
    let lines = items
        .iter()
        .map(|item| item.description.clone())
        .collect::<Vec<_>>();

    if lines.is_empty() {
        "No menu items found".to_string()
    } else {
        lines.join("\n")
    }
}

fn render_no_lunch_reason(reason: &NoLunchReason) -> &'static str {
    match reason {
        NoLunchReason::Weekend => "weekend",
        NoLunchReason::Closed => "closed",
        NoLunchReason::MissingDay => "missing day in menu",
        NoLunchReason::EmptyMenu => "empty menu",
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
        SourceError::InvalidPrice(price) => format!("invalid price: {price}"),
        SourceError::UnsupportedFormat(message) => format!("unsupported format: {message}"),
    }
}

impl std::fmt::Display for crate::domain::Currency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sek => f.write_str("SEK"),
        }
    }
}

#[allow(dead_code)]
fn _keep_weekday_import_used(_: Weekday) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{RestaurantId, RestaurantMeta, SourceKind};

    #[test]
    fn renders_stub_source_state() {
        let lunch = RestaurantLunch {
            meta: RestaurantMeta {
                id: RestaurantId::JinxEmpire,
                display_name: "Jinx Empire",
                source_url: "https://www.jinxempire.com/#menu",
                source_kind: SourceKind::JsonLdMenu,
            },
            state: LunchState::Unavailable {
                stage: FailureStage::Parse,
                error: SourceError::NotImplemented,
            },
        };

        let rendered = render_day(Weekday::Wednesday, &[lunch]);

        assert!(rendered.contains("Today (Wednesday)"));
        assert!(rendered.contains("Jinx Empire"));
        assert!(rendered.contains("parser not implemented yet"));
    }
}
