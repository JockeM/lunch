use crate::date::Weekday;
use crate::domain::{LunchItem, LunchState, RestaurantId, RestaurantLunch};
use serde_json::json;

use super::{order_link, render_error, render_no_lunch_reason, render_stage};

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
    let mut state = render_slack_state(&lunch.state, lunch.meta.id);
    if let Some((url, label)) = order_link(lunch) {
        state.push('\n');
        state.push_str(&slack_link(url, label));
    }

    json!({
        "type": "section",
        "text": {
            "type": "mrkdwn",
            "text": format!(
                "*{}*\n{}",
                slack_link(lunch.meta.source_url, lunch.meta.display_name),
                state,
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

fn render_slack_state(state: &LunchState, restaurant: RestaurantId) -> String {
    match state {
        LunchState::Available { items, notes, .. } => {
            let mut lines = match restaurant {
                RestaurantId::Gabys => render_grouped_available(
                    items,
                    &[("Green Kitchen", true), ("Poke Bowl", false)],
                ),
                RestaurantId::Svinn => {
                    render_grouped_available(items, &[("Extra hos Svinn", true)])
                }
                RestaurantId::Vallagat => {
                    render_grouped_available(items, &[("Pick & Weigh", false)])
                }
                RestaurantId::JinxEmpire => render_slack_available(items),
            };

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

fn render_grouped_available(items: &[LunchItem], groups: &[(&str, bool)]) -> Vec<String> {
    if items.is_empty() {
        return render_slack_available(items);
    }

    let mut dishes = vec![Vec::new(); groups.len() + 1];
    for item in items {
        let group = groups
            .iter()
            .enumerate()
            .find_map(|(index, (heading, bullets))| {
                item.description
                    .strip_prefix(heading)
                    .and_then(|text| text.strip_prefix(": "))
                    .map(|text| (index + 1, *bullets, text))
            });
        let (index, bullets, text) = group.unwrap_or((0, true, &item.description));
        let text = slack_escape(text);
        dishes[index].push(if bullets { format!("• {text}") } else { text });
    }

    let mut sections = Vec::new();
    let headings =
        std::iter::once("Today's lunch").chain(groups.iter().map(|(heading, _)| *heading));
    for (heading, dishes) in headings.zip(dishes) {
        if !dishes.is_empty() {
            sections.push(format!("*{heading}*\n{}", dishes.join("\n")));
        }
    }

    vec![format!("\n{}", sections.join("\n\n"))]
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
    slack_escape(&item.description)
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
    use crate::domain::{RestaurantId, RestaurantMeta, SourceKind};

    #[test]
    fn renders_vallagat_buffet_as_a_separate_section() {
        let state = LunchState::Available {
            weekday: Weekday::Monday,
            items: [
                "Kycklingsaltimbocca",
                "Pick & Weigh: Salladsbuffé – betala efter vikt",
            ]
            .into_iter()
            .map(|description| LunchItem {
                description: description.to_string(),
            })
            .collect(),
            notes: Vec::new(),
        };
        assert_eq!(
            render_slack_state(&state, RestaurantId::Vallagat),
            "\n*Today's lunch*\n• Kycklingsaltimbocca\n\n*Pick & Weigh*\nSalladsbuffé – betala efter vikt"
        );
    }

    #[test]
    fn groups_svinn_extras_under_one_heading() {
        let state = LunchState::Available {
            weekday: Weekday::Monday,
            items: [
                "Pannbiff",
                "Extra hos Svinn: Köttbullar, 119 kr",
                "Extra hos Svinn: Quinoasallad (Först till kvarn)",
            ]
            .into_iter()
            .map(|description| LunchItem {
                description: description.to_string(),
            })
            .collect(),
            notes: Vec::new(),
        };

        assert_eq!(
            render_slack_state(&state, RestaurantId::Svinn),
            "\n*Today's lunch*\n• Pannbiff\n\n*Extra hos Svinn*\n• Köttbullar, 119 kr\n• Quinoasallad (Först till kvarn)"
        );
    }

    #[test]
    fn groups_gabys_dishes_under_slack_headings() {
        let lunch = RestaurantLunch {
            meta: RestaurantMeta {
                id: RestaurantId::Gabys,
                display_name: "Gaby's",
                source_url: "https://jacyzhotel.com/restauranger-goteborg/gabys/#lunch",
                source_kind: SourceKind::HtmlWeekdayText,
            },
            state: LunchState::Available {
                weekday: Weekday::Friday,
                items: [
                    "Halstrad lax & potatis",
                    "Gnocchi, tryffelsås",
                    "Green Kitchen: Caesarsallad",
                    "Green Kitchen: Caesar West Coast",
                    "Poke Bowl: Glasnudlar, lax",
                ]
                .into_iter()
                .map(|description| LunchItem {
                    description: description.to_string(),
                })
                .collect(),
                notes: Vec::new(),
            },
        };

        let payload: serde_json::Value =
            serde_json::from_str(&render_slack_payload(Weekday::Friday, &[lunch])).unwrap();

        assert_eq!(
            payload["blocks"][2]["text"]["text"],
            "*<https://jacyzhotel.com/restauranger-goteborg/gabys/#lunch|Gaby's>*\n\n\
             *Today's lunch*\n• Halstrad lax &amp; potatis\n• Gnocchi, tryffelsås\n\n\
             *Green Kitchen*\n• Caesarsallad\n• Caesar West Coast\n\n\
             *Poke Bowl*\nGlasnudlar, lax"
        );
    }

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
                }],
                notes: Vec::new(),
            },
        };

        let rendered = render_slack_payload(Weekday::Wednesday, std::slice::from_ref(&lunch));
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
                .contains("Bibimbap &amp; greens")
        );
        let order = "<https://order.openpos.cloud/jinxempire/019a76e1-5979-7d97-b49a-4a9dd57dac33?label=666|Order online — 10 kr off>";
        assert!(
            payload["blocks"][2]["text"]["text"]
                .as_str()
                .unwrap()
                .ends_with(&format!("\n{order}"))
        );

        let mut lunch = lunch;
        lunch.meta.id = RestaurantId::Gabys;
        assert!(!render_lunch_block(&lunch).to_string().contains(order));

        lunch.meta.id = RestaurantId::JinxEmpire;
        for state in [
            LunchState::NoLunchToday {
                weekday: Weekday::Wednesday,
                reason: crate::domain::NoLunchReason::Closed,
            },
            LunchState::Unavailable {
                stage: crate::domain::FailureStage::Fetch,
                error: crate::domain::SourceError::NotImplemented,
            },
        ] {
            lunch.state = state;
            assert!(!render_lunch_block(&lunch).to_string().contains(order));
        }
    }
}
