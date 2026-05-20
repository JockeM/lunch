#![allow(dead_code)]

use crate::date::Weekday;
use crate::domain::{
    Currency, FailureStage, LunchItem, LunchState, NoLunchReason, Price, RestaurantId,
    RestaurantMeta, SourceError, SourceKind,
};
use crate::restaurants::RestaurantSource;
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

    fn lunch_for(&self, weekday: Weekday) -> LunchState {
        match fetch_body(self.meta().source_url) {
            Ok(body) => {
                parse_lunch(&body, weekday).unwrap_or_else(|error| LunchState::Unavailable {
                    stage: FailureStage::Parse,
                    error,
                })
            }
            Err(error) => LunchState::Unavailable {
                stage: FailureStage::Fetch,
                error,
            },
        }
    }
}

fn fetch_body(url: &str) -> Result<String, SourceError> {
    let response = reqwest::blocking::Client::builder()
        .user_agent("lunch/0.1")
        .build()
        .map_err(|error| SourceError::Network(error.to_string()))?
        .get(url)
        .send()
        .map_err(|error| SourceError::Network(error.to_string()))?;
    let status = response.status();

    if !status.is_success() {
        return Err(SourceError::HttpStatus(status.as_u16()));
    }

    response
        .text()
        .map_err(|error| SourceError::Network(error.to_string()))
}

pub fn parse_lunch(body: &str, weekday: Weekday) -> Result<LunchState, SourceError> {
    match parse_structured_lunch(body, weekday) {
        Ok(lunch) => Ok(lunch),
        Err(SourceError::MissingStructuredData) => parse_visible_lunch(body, weekday),
        Err(error) => Err(error),
    }
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
            price: parse_price(item)?,
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
        .map(|position| item_start + 1 + position)
        .unwrap_or(lines.len());
    let mut block = lines[item_start + 1..next_day].iter();
    let first = block
        .next()
        .ok_or(SourceError::MissingExpectedElement("lunch description"))?;
    let (price, description) = if let Some(price) = parse_price_text(first)? {
        let description = block
            .next()
            .ok_or(SourceError::MissingExpectedElement("lunch description"))?;

        (Some(price), description)
    } else {
        (None, first)
    };

    Ok(LunchState::Available {
        weekday,
        items: vec![LunchItem {
            description: description.to_string(),
            price,
        }],
        notes: Vec::new(),
    })
}

fn visible_text_lines(body: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut text = String::new();
    let mut in_tag = false;

    for character in body.chars() {
        match character {
            '<' => {
                push_text_line(&mut lines, &mut text);
                in_tag = true;
            }
            '>' => in_tag = false,
            _ if !in_tag => text.push(character),
            _ => {}
        }
    }

    push_text_line(&mut lines, &mut text);
    lines
}

fn push_text_line(lines: &mut Vec<String>, text: &mut String) {
    let line = decode_html_entities(text).trim().to_string();

    if !line.is_empty() {
        lines.push(line);
    }

    text.clear();
}

fn decode_html_entities(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&nbsp;", " ")
        .replace("&#x27;", "'")
        .replace("&quot;", "\"")
}

fn parse_price_text(value: &str) -> Result<Option<Price>, SourceError> {
    let normalized = value.trim().to_ascii_lowercase();
    let Some(amount) = normalized.strip_suffix(" kr") else {
        return Ok(None);
    };
    let amount = amount
        .trim()
        .parse::<u32>()
        .map_err(|_| SourceError::InvalidPrice(value.to_string()))?;

    Ok(Some(Price {
        amount,
        currency: Currency::Sek,
    }))
}

fn parse_menu_json(body: &str) -> Result<Value, SourceError> {
    parse_json_candidates(body)
        .into_iter()
        .find_map(|candidate| serde_json::from_str::<Value>(candidate).ok())
        .and_then(find_menu)
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

        if tag.contains("application/ld+json") {
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
        Value::Object(mut object) => {
            if let Some(graph) = object.remove("@graph") {
                find_menu(graph)
            } else {
                None
            }
        }
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

fn parse_price(item: &Value) -> Result<Option<Price>, SourceError> {
    let Some(offer) = item.get("offers") else {
        return Ok(None);
    };
    let Some(price) = offer.get("price").and_then(Value::as_str) else {
        return Ok(None);
    };
    let amount = price
        .trim()
        .parse::<u32>()
        .map_err(|_| SourceError::InvalidPrice(price.to_string()))?;
    let currency = match offer
        .get("priceCurrency")
        .and_then(Value::as_str)
        .unwrap_or("SEK")
    {
        "SEK" => Currency::Sek,
        currency => {
            return Err(SourceError::UnsupportedFormat(format!(
                "currency {currency}"
            )));
        }
    };

    Ok(Some(Price { amount, currency }))
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
        let lunch = parse_lunch(MENU_JSON, Weekday::Wednesday).unwrap();

        assert_eq!(
            lunch,
            LunchState::Available {
                weekday: Weekday::Wednesday,
                items: vec![LunchItem {
                    description: "Ground Pork or Plant-Based Mince / Sambal / Coconut Rice"
                        .to_string(),
                    price: Some(Price {
                        amount: 135,
                        currency: Currency::Sek,
                    }),
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

        let lunch = parse_lunch(&body, Weekday::Monday).unwrap();

        assert!(matches!(lunch, LunchState::Available { .. }));
    }

    #[test]
    fn parses_visible_lunch_section_when_json_ld_is_missing() {
        let body = r#"
            <html>
              <body>
                <h3>Lunch</h3>
                <p>W.21</p>
                <h4>Monday</h4>
                <p>135 kr</p>
                <p>Pork Belly or Tofu / Plum Glaze / Garlic Mayo</p>
                <h4>Wednesday</h4>
                <p>135 kr</p>
                <p>Ground Pork or Plant-Based Mince / Sambal / Coconut Rice</p>
                <h4>Thursday</h4>
                <p>Beef Chuck or Portobello / Smoky Hoisin</p>
              </body>
            </html>
        "#;

        let lunch = parse_lunch(body, Weekday::Wednesday).unwrap();

        assert_eq!(
            lunch,
            LunchState::Available {
                weekday: Weekday::Wednesday,
                items: vec![LunchItem {
                    description: "Ground Pork or Plant-Based Mince / Sambal / Coconut Rice"
                        .to_string(),
                    price: Some(Price {
                        amount: 135,
                        currency: Currency::Sek,
                    }),
                }],
                notes: Vec::new(),
            }
        );
    }

    #[test]
    fn errors_when_structured_menu_is_missing() {
        assert_eq!(
            parse_lunch("{}", Weekday::Monday),
            Err(SourceError::MissingStructuredData)
        );
    }
}
