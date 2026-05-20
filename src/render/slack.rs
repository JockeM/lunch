use crate::date::Weekday;
use crate::domain::{LunchItem, LunchState, RestaurantLunch};
use serde_json::json;

use super::{render_error, render_no_lunch_reason, render_price, render_stage};

pub fn render_slack_payload(weekday: Weekday, lunches: &[RestaurantLunch]) -> String {
    let mut blocks = vec![json!({
        "type": "header",
        "text": {
            "type": "plain_text",
            "text": format!("Lunch today ({weekday})"),
            "emoji": true,
        },
    })];

    for lunch in lunches {
        blocks.push(json!({ "type": "divider" }));
        blocks.push(render_lunch_block(lunch));
    }

    json!({
        "text": render_slack_fallback(weekday, lunches),
        "blocks": blocks,
    })
    .to_string()
}

fn render_lunch_block(lunch: &RestaurantLunch) -> serde_json::Value {
    json!({
        "type": "section",
        "text": {
            "type": "mrkdwn",
            "text": format!(
                "*{}*\n{}",
                slack_link(lunch.meta.source_url, lunch.meta.display_name),
                render_slack_state(&lunch.state),
            ),
        },
    })
}

fn render_slack_fallback(weekday: Weekday, lunches: &[RestaurantLunch]) -> String {
    let restaurants = lunches
        .iter()
        .map(|lunch| lunch.meta.display_name)
        .collect::<Vec<_>>();

    if restaurants.is_empty() {
        format!("Lunch today ({weekday})")
    } else {
        format!("Lunch today ({weekday}): {}", restaurants.join(", "))
    }
}

fn render_slack_state(state: &LunchState) -> String {
    match state {
        LunchState::Available { items, notes, .. } => {
            let mut lines = render_slack_available(items);

            lines.extend(notes.iter().map(|note| format!("_{}_", slack_escape(note))));
            lines.join("\n")
        }
        LunchState::NoLunchToday {
            weekday, reason, ..
        } => {
            format!(
                "_No lunch for {weekday}: {}_",
                slack_escape(render_no_lunch_reason(reason))
            )
        }
        LunchState::Unavailable { stage, error } => {
            format!(
                "_Unavailable: {} error, {}_",
                slack_escape(render_stage(*stage)),
                slack_escape(&render_error(error))
            )
        }
    }
}

fn render_slack_available(items: &[LunchItem]) -> Vec<String> {
    if items.is_empty() {
        return vec!["_No menu items found_".to_string()];
    }

    items
        .iter()
        .map(|item| format!("- {}", render_slack_item(item)))
        .collect()
}

fn render_slack_item(item: &LunchItem) -> String {
    match &item.price {
        Some(price) => format!(
            "{} _{}_",
            slack_escape(&item.description),
            slack_escape(&render_price(price))
        ),
        None => slack_escape(&item.description),
    }
}

fn slack_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn slack_link(url: &str, label: &str) -> String {
    format!("<{}|{}>", slack_escape(url), slack_escape(label))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Currency, Price, RestaurantId, RestaurantMeta, SourceKind};

    #[test]
    fn renders_slack_payload() {
        let lunch = RestaurantLunch {
            meta: RestaurantMeta {
                id: RestaurantId::JinxEmpire,
                display_name: "Jinx Empire",
                source_url: "https://www.jinxempire.com/#menu",
                source_kind: SourceKind::JsonLdMenu,
            },
            state: LunchState::Available {
                weekday: Weekday::Wednesday,
                items: vec![LunchItem {
                    description: "Bibimbap & greens".to_string(),
                    price: Some(Price {
                        amount: 135,
                        currency: Currency::Sek,
                    }),
                }],
                notes: Vec::new(),
            },
        };

        let rendered = render_slack_payload(Weekday::Wednesday, &[lunch]);
        let payload: serde_json::Value = serde_json::from_str(&rendered).unwrap();

        assert_eq!(payload["text"], "Lunch today (Wednesday): Jinx Empire");
        assert_eq!(payload["blocks"][0]["type"], "header");
        assert_eq!(payload["blocks"][1]["type"], "divider");
        assert_eq!(payload["blocks"][2]["type"], "section");
        assert!(payload["blocks"][2]["accessory"].is_null());
        assert!(
            payload["blocks"][2]["text"]["text"]
                .as_str()
                .unwrap()
                .contains("*<https://www.jinxempire.com/#menu|Jinx Empire>*")
        );
        assert!(
            payload["blocks"][2]["text"]["text"]
                .as_str()
                .unwrap()
                .contains("Bibimbap &amp; greens _135 SEK_")
        );
    }
}
