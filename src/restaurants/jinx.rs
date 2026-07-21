#![allow(dead_code)]

use crate::date::{Date, Weekday};
use crate::domain::{
    FailureStage, LunchItem, LunchState, NoLunchReason, RestaurantId, RestaurantMeta, SourceError,
    SourceKind,
};
use crate::restaurants::{
    RestaurantSource,
    utils::{fetch_body, visible_text_lines},
};
use serde_json::Value;

pub struct JinxEmpire;

impl RestaurantSource for JinxEmpire {
    fn meta(&self) -> RestaurantMeta {
        RestaurantMeta {
            id: RestaurantId::JinxEmpire,
            display_name: "Jinx Empire",
            source_url: "https://www.jinxempire.com/#menu",
            source_kind: SourceKind::JsonLdMenu,
        }
    }

    fn lunch_for(&self, date: Date) -> LunchState {
        match fetch_body(self.meta().source_url) {
            Ok(body) => parse_lunch(&body, date).unwrap_or_else(|error| LunchState::Unavailable {
                stage: FailureStage::Parse,
                error,
            }),
            Err(error) => LunchState::Unavailable {
                stage: FailureStage::Fetch,
                error,
            },
        }
    }
}

pub fn parse_lunch(body: &str, date: Date) -> Result<LunchState, SourceError> {
    if is_summer_break(body, date.iso_week) {
        return Ok(LunchState::NoLunchToday {
            weekday: date.weekday,
            reason: NoLunchReason::Closed,
        });
    }

    match parse_structured_lunch(body, date.weekday) {
        Ok(lunch) => Ok(lunch),
        Err(SourceError::MissingStructuredData) => parse_visible_lunch(body, date.weekday),
        Err(error) => Err(error),
    }
}

fn is_summer_break(body: &str, current_week: u8) -> bool {
    let body = body.to_ascii_lowercase();
    let Some(break_text) = body
        .split_once("summer break from week ")
        .map(|(_, break_text)| break_text)
    else {
        return false;
    };
    let Some(start_week) = parse_leading_week(break_text) else {
        return false;
    };
    let Some(return_text) = break_text
        .find("return")
        .map(|return_start| &break_text[return_start..])
    else {
        return false;
    };
    let Some(return_week) = return_text
        .split_once("week ")
        .and_then(|(_, week_text)| parse_leading_week(week_text))
    else {
        return false;
    };

    if start_week < return_week {
        (start_week..return_week).contains(&current_week)
    } else {
        current_week >= start_week || current_week < return_week
    }
}

fn parse_leading_week(value: &str) -> Option<u8> {
    let digits = value
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    let week = digits.parse::<u8>().ok()?;

    (1..=53).contains(&week).then_some(week)
}

fn parse_structured_lunch(body: &str, weekday: Weekday) -> Result<LunchState, SourceError> {
    let menu = parse_menu_json(body)?;
    let lunch_section = menu
        .get("hasMenuSection")
        .and_then(Value::as_array)
        .and_then(|sections| {
            sections
                .iter()
                .find(|section| section_name_is(section, "Lunch"))
        })
        .ok_or(SourceError::MissingExpectedElement("Lunch menu section"))?;
    let items = lunch_section
        .get("hasMenuItem")
        .and_then(Value::as_array)
        .ok_or(SourceError::MissingExpectedElement("Lunch menu items"))?;

    let item = items
        .iter()
        .find(|item| item_weekday(item) == Some(weekday))
        .ok_or_else(|| {
            SourceError::UnsupportedFormat(format!("missing menu item for {weekday}"))
        })?;
    let description = item
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|description| !description.is_empty())
        .ok_or(SourceError::MissingExpectedElement("MenuItem.description"))?;

    Ok(LunchState::Available {
        weekday,
        items: vec![LunchItem {
            description: description.to_string(),
        }],
        notes: Vec::new(),
    })
}

fn parse_visible_lunch(body: &str, weekday: Weekday) -> Result<LunchState, SourceError> {
    if matches!(weekday, Weekday::Saturday | Weekday::Sunday) {
        return Ok(LunchState::NoLunchToday {
            weekday,
            reason: NoLunchReason::Weekend,
        });
    }

    let lines = visible_text_lines(body);
    let lunch_start = lines
        .iter()
        .rposition(|line| line.eq_ignore_ascii_case("Lunch"))
        .ok_or(SourceError::MissingStructuredData)?;
    let Some(item_start) = lines[lunch_start..]
        .iter()
        .position(|line| parse_weekday(line) == Some(weekday))
        .map(|position| lunch_start + position)
    else {
        return Ok(LunchState::NoLunchToday {
            weekday,
            reason: NoLunchReason::MissingDay,
        });
    };
    let next_day = lines[item_start + 1..]
        .iter()
        .position(|line| parse_weekday(line).is_some())
        .map_or(lines.len(), |position| item_start + 1 + position);
    let description = lines[item_start + 1..next_day]
        .iter()
        .filter(|line| !is_price_line(line))
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(" ");
    let description = (!description.is_empty())
        .then_some(description)
        .ok_or(SourceError::MissingExpectedElement("lunch description"))?;
    Ok(LunchState::Available {
        weekday,
        items: vec![LunchItem { description }],
        notes: Vec::new(),
    })
}

fn parse_menu_json(body: &str) -> Result<Value, SourceError> {
    parse_json_candidates(body)
        .into_iter()
        .find_map(|candidate| serde_json::from_str::<Value>(candidate).ok().and_then(find_menu))
        .ok_or(SourceError::MissingStructuredData)
}

fn parse_json_candidates(body: &str) -> Vec<&str> {
    let mut candidates = extract_json_ld_scripts(body);

    candidates.push(body.trim());
    candidates
}

fn extract_json_ld_scripts(body: &str) -> Vec<&str> {
    let mut scripts = Vec::new();
    let mut offset = 0;

    while let Some(script_start) = body[offset..].find("<script") {
        let script_start = offset + script_start;
        let Some(tag_end) = body[script_start..].find('>') else {
            break;
        };
        let tag_end = script_start + tag_end;
        let tag = &body[script_start..=tag_end];
        let content_start = tag_end + 1;
        let Some(script_end) = body[content_start..].find("</script>") else {
            break;
        };
        let script_end = content_start + script_end;

        if tag.contains("application/ld+json") || tag.contains("application/json") {
            scripts.push(body[content_start..script_end].trim());
        }

        offset = script_end + "</script>".len();
    }

    scripts
}

fn find_menu(value: Value) -> Option<Value> {
    if is_type(&value, "Menu") {
        return Some(value);
    }

    match value {
        Value::Array(values) => values.into_iter().find_map(find_menu),
        Value::Object(object) => object.into_values().find_map(find_menu),
        _ => None,
    }
}

fn section_name_is(section: &Value, expected_name: &str) -> bool {
    section
        .get("name")
        .and_then(Value::as_str)
        .is_some_and(|name| name.eq_ignore_ascii_case(expected_name))
}

fn item_weekday(item: &Value) -> Option<Weekday> {
    item.get("name")
        .and_then(Value::as_str)
        .and_then(parse_weekday)
}

fn parse_weekday(value: &str) -> Option<Weekday> {
    match value.trim().to_ascii_lowercase().as_str() {
        "monday" => Some(Weekday::Monday),
        "tuesday" => Some(Weekday::Tuesday),
        "wednesday" => Some(Weekday::Wednesday),
        "thursday" => Some(Weekday::Thursday),
        "friday" => Some(Weekday::Friday),
        "saturday" => Some(Weekday::Saturday),
        "sunday" => Some(Weekday::Sunday),
        _ => None,
    }
}

fn is_price_line(value: &str) -> bool {
    let value = value.trim().to_ascii_lowercase();
    let Some(price) = value.strip_suffix(" kr") else {
        return false;
    };

    !price.is_empty() && price.chars().all(|character| character.is_ascii_digit())
}

fn is_type(value: &Value, expected_type: &str) -> bool {
    match value.get("@type") {
        Some(Value::String(actual_type)) => actual_type == expected_type,
        Some(Value::Array(types)) => types
            .iter()
            .any(|actual_type| actual_type.as_str() == Some(expected_type)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(weekday: Weekday) -> Date {
        Date {
            iso_year: 2026,
            iso_week: 21,
            weekday,
        }
    }

    const MENU_JSON: &str = r#"{
        "@context": "https://schema.org",
        "@type": "Menu",
        "name": "Jinx Empire Menu",
        "hasMenuSection": [
          {
            "@type": "MenuSection",
            "name": "Lunch",
            "hasMenuItem": [
              {
                "@type": "MenuItem",
                "name": "Monday",
                "description": "Pork Belly or Tofu / Plum Glaze / Garlic Mayo",
                "offers": {
                  "@type": "Offer",
                  "price": "135",
                  "priceCurrency": "SEK"
                }
              },
              {
                "@type": "MenuItem",
                "name": "Wednesday",
                "description": "Ground Pork or Plant-Based Mince / Sambal / Coconut Rice",
                "offers": {
                  "@type": "Offer",
                  "price": "135",
                  "priceCurrency": "SEK"
                }
              }
            ]
          }
        ]
    }"#;

    #[test]
    fn parses_raw_menu_json_for_weekday() {
        let lunch = parse_lunch(MENU_JSON, date(Weekday::Wednesday)).unwrap();

        assert_eq!(
            lunch,
            LunchState::Available {
                weekday: Weekday::Wednesday,
                items: vec![LunchItem {
                    description: "Ground Pork or Plant-Based Mince / Sambal / Coconut Rice"
                        .to_string(),
                }],
                notes: Vec::new(),
            }
        );
    }

    #[test]
    fn parses_html_json_ld_script() {
        let body = format!(
            r#"<html><head><script type="application/ld+json">{MENU_JSON}</script></head></html>"#
        );

        let lunch = parse_lunch(&body, date(Weekday::Monday)).unwrap();

        assert!(matches!(lunch, LunchState::Available { .. }));
    }

    #[test]
    fn skips_non_menu_json_ld_scripts() {
        let body = format!(
            r#"
            <html>
              <head>
                <script type="application/ld+json">{{"@context":"https://schema.org","@type":"WebSite","name":"Jinx Empire"}}</script>
                <script type="application/ld+json">{MENU_JSON}</script>
              </head>
            </html>
            "#
        );

        let lunch = parse_lunch(&body, date(Weekday::Wednesday)).unwrap();

        assert_eq!(
            lunch,
            LunchState::Available {
                weekday: Weekday::Wednesday,
                items: vec![LunchItem {
                    description: "Ground Pork or Plant-Based Mince / Sambal / Coconut Rice"
                        .to_string(),
                }],
                notes: Vec::new(),
            }
        );
    }

    #[test]
    fn parses_visible_lunch_section_when_json_ld_is_missing() {
        let body = r#"
            <html>
              <body>
                <h3>Lunch</h3>
                <p>W.21</p>
                <h4>Monday</h4>
                <p>Pork Belly or Tofu / Plum Glaze / Garlic Mayo</p>
                <h4>Wednesday</h4>
                <p>Ground Pork or Plant-Based Mince / Sambal / Coconut Rice</p>
                <h4>Thursday</h4>
                <p>Beef Chuck or Portobello / Smoky Hoisin</p>
              </body>
            </html>
        "#;

        let lunch = parse_lunch(body, date(Weekday::Wednesday)).unwrap();

        assert_eq!(
            lunch,
            LunchState::Available {
                weekday: Weekday::Wednesday,
                items: vec![LunchItem {
                    description: "Ground Pork or Plant-Based Mince / Sambal / Coconut Rice"
                        .to_string(),
                }],
                notes: Vec::new(),
            }
        );
    }

    #[test]
    fn parses_visible_lunch_when_price_precedes_description() {
        let body = r#"
            <html>
              <body>
                <h3>Lunch</h3>
                <h4>Monday</h4>
                <p>135 kr</p>
                <p>Pork Belly or Tofu / Plum Glaze / Garlic Mayo</p>
                <h4>Tuesday</h4>
                <p>Chicken or Cauliflower / Red Curry</p>
              </body>
            </html>
        "#;

        let lunch = parse_lunch(body, date(Weekday::Monday)).unwrap();

        assert_eq!(
            lunch,
            LunchState::Available {
                weekday: Weekday::Monday,
                items: vec![LunchItem {
                    description: "Pork Belly or Tofu / Plum Glaze / Garlic Mayo".to_string(),
                }],
                notes: Vec::new(),
            }
        );
    }

    #[test]
    fn parses_visible_lunch_when_description_is_split_by_formatting_tags() {
        let body = r#"
            <html>
              <body>
                <h3>Lunch</h3>
                <h4>Tuesday</h4>
                <span>135 kr</span>
                <p><b>Pork or tofu</b> / Hung Lay curry / Carrot salad / Cilantro / Crispy cashews</p>
                <h4>Wednesday</h4>
                <p><b>Chicken or pointed cabbage</b> / Lemongrass jus</p>
              </body>
            </html>
        "#;

        let lunch = parse_lunch(body, date(Weekday::Tuesday)).unwrap();

        assert_eq!(
            lunch,
            LunchState::Available {
                weekday: Weekday::Tuesday,
                items: vec![LunchItem {
                    description:
                        "Pork or tofu / Hung Lay curry / Carrot salad / Cilantro / Crispy cashews"
                            .to_string(),
                }],
                notes: Vec::new(),
            }
        );
    }

    #[test]
    fn finds_menu_nested_in_next_data() {
        let body = format!(
            r#"<html><body><script id="__NEXT_DATA__" type="application/json">{{"props":{{"pageProps":{{"structuredData":[{MENU_JSON}]}}}}}}</script></body></html>"#
        );

        let lunch = parse_lunch(&body, date(Weekday::Monday)).unwrap();

        assert_eq!(
            lunch,
            LunchState::Available {
                weekday: Weekday::Monday,
                items: vec![LunchItem {
                    description: "Pork Belly or Tofu / Plum Glaze / Garlic Mayo".to_string(),
                }],
                notes: Vec::new(),
            }
        );
    }

    #[test]
    fn errors_when_structured_menu_is_missing() {
        assert_eq!(
            parse_lunch("{}", date(Weekday::Monday)),
            Err(SourceError::MissingStructuredData)
        );
    }

    #[test]
    fn returns_closed_during_published_summer_break() {
        let body = format!(
            r#"<script type="application/ld+json">{{
                "description": "Our Lunch service is on summer break from Week 28 and will return on Monday of Week 33.",
                "menu": {MENU_JSON}
            }}</script>"#
        );

        assert_eq!(
            parse_lunch(
                &body,
                Date {
                    iso_year: 2026,
                    iso_week: 30,
                    weekday: Weekday::Tuesday,
                }
            ),
            Ok(LunchState::NoLunchToday {
                weekday: Weekday::Tuesday,
                reason: NoLunchReason::Closed,
            })
        );
    }

    #[test]
    fn parses_menu_after_published_summer_break() {
        let body = format!(
            r#"<script type="application/ld+json">{{
                "description": "Our Lunch service is on summer break from Week 28 and will return on Monday of Week 33.",
                "menu": {MENU_JSON}
            }}</script>"#
        );
        let lunch = parse_lunch(
            &body,
            Date {
                iso_year: 2026,
                iso_week: 33,
                weekday: Weekday::Monday,
            },
        )
        .unwrap();

        assert!(matches!(lunch, LunchState::Available { .. }));
    }
}
