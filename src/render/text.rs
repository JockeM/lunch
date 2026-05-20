use crate::date::Weekday;
use crate::domain::{LunchItem, LunchState, RestaurantLunch};

use super::{render_error, render_no_lunch_reason, render_price, render_stage};

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
    let lines = items.iter().map(render_item).collect::<Vec<_>>();

    if lines.is_empty() {
        "No menu items found".to_string()
    } else {
        lines.join("\n")
    }
}

fn render_item(item: &LunchItem) -> String {
    match &item.price {
        Some(price) => format!("{} ({})", item.description, render_price(price)),
        None => item.description.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{FailureStage, RestaurantId, RestaurantMeta, SourceError, SourceKind};

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
