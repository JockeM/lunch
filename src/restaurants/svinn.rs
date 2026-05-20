use crate::date::Weekday;
use crate::domain::{
    Currency, FailureStage, LunchItem, LunchState, NoLunchReason, Price, RestaurantId,
    RestaurantMeta, SourceError, SourceKind,
};
use crate::restaurants::RestaurantSource;

pub struct Svinn;

impl RestaurantSource for Svinn {
    fn meta(&self) -> RestaurantMeta {
        RestaurantMeta {
            id: RestaurantId::Svinn,
            display_name: "Restaurang Svinn",
            source_url: "https://svinn.kvartersmenyn.se/",
            source_kind: SourceKind::HtmlWeekdayText,
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
    if matches!(weekday, Weekday::Saturday | Weekday::Sunday) {
        return Ok(LunchState::NoLunchToday {
            weekday,
            reason: NoLunchReason::Weekend,
        });
    }

    let price = parse_global_price(body);
    let menu_lines = menu_lines(body)?;
    let day_lines = find_weekday_lines(&menu_lines, weekday);

    if day_lines.is_empty() {
        return Ok(LunchState::NoLunchToday {
            weekday,
            reason: NoLunchReason::MissingDay,
        });
    }

    let items = parse_items(day_lines, price);

    if items.is_empty() {
        return Ok(LunchState::NoLunchToday {
            weekday,
            reason: NoLunchReason::EmptyMenu,
        });
    }

    Ok(LunchState::Available {
        weekday,
        items,
        notes: Vec::new(),
    })
}

fn menu_lines(body: &str) -> Result<Vec<String>, SourceError> {
    let Some(lunch_menu_start) = body.find("Lunchmeny") else {
        return Err(SourceError::MissingExpectedElement("Lunchmeny"));
    };
    let Some(menu_start) = body[lunch_menu_start..].find("<div class=\"meny\"") else {
        return Err(SourceError::MissingExpectedElement("menu div"));
    };
    let menu_start = lunch_menu_start + menu_start;
    let menu_body = &body[menu_start..];

    Ok(visible_text_lines(&strip_hidden_garbage(menu_body)))
}

fn find_weekday_lines(lines: &[String], weekday: Weekday) -> Vec<String> {
    let Some(day_start) = lines
        .iter()
        .position(|line| parse_swedish_weekday(line) == Some(weekday))
    else {
        return Vec::new();
    };
    let day_end = lines[day_start + 1..]
        .iter()
        .position(|line| parse_swedish_weekday(line).is_some() || is_non_day_section(line))
        .map(|position| day_start + 1 + position)
        .unwrap_or(lines.len());

    lines[day_start + 1..day_end].to_vec()
}

fn parse_items(lines: Vec<String>, price: Option<Price>) -> Vec<LunchItem> {
    let lines = lines
        .into_iter()
        .filter(|line| is_dish_line(line))
        .collect::<Vec<_>>();

    match lines.as_slice() {
        [] => Vec::new(),
        [single] => vec![lunch_item(single, price)],
        [first, second] => vec![
            lunch_item(first, price.clone()),
            lunch_item(second, price.clone()),
        ],
        lines => split_two_items(lines)
            .into_iter()
            .map(|description| lunch_item(&description, price.clone()))
            .collect(),
    }
}

fn split_two_items(lines: &[String]) -> Vec<String> {
    let second_title = (1..lines.len())
        .find(|&index| starts_with_uppercase(&lines[index]) && index >= lines.len() / 2)
        .unwrap_or(lines.len());

    [
        join_lines(&lines[..second_title]),
        join_lines(&lines[second_title..]),
    ]
    .into_iter()
    .filter(|description| !description.is_empty())
    .collect()
}

fn lunch_item(description: &str, price: Option<Price>) -> LunchItem {
    LunchItem {
        description: join_sentence(description),
        price,
    }
}

fn join_lines(lines: &[String]) -> String {
    join_sentence(&lines.join(" "))
}

fn join_sentence(value: &str) -> String {
    normalize_text(value)
}

fn starts_with_uppercase(value: &str) -> bool {
    value
        .chars()
        .find(|character| character.is_alphabetic())
        .is_some_and(|character| character.is_uppercase())
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
    let line = normalize_text(&decode_html_entities(text));

    if !line.is_empty() {
        lines.push(line);
    }

    text.clear();
}

fn normalize_text(value: &str) -> String {
    value
        .replace('\u{200b}', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn strip_hidden_garbage(body: &str) -> String {
    let mut stripped = String::new();
    let mut rest = body;

    while let Some(start) = rest.find("<i") {
        stripped.push_str(&rest[..start]);
        let tag_and_after = &rest[start..];
        let Some(tag_end) = tag_and_after.find('>') else {
            rest = "";
            break;
        };
        let tag = &tag_and_after[..=tag_end];

        if tag.contains("opacity: 0.1") {
            if let Some(end) = tag_and_after[tag_end + 1..].find("</i>") {
                rest = &tag_and_after[tag_end + 1 + end + "</i>".len()..];
            } else {
                rest = "";
                break;
            }
        } else {
            stripped.push_str(tag);
            rest = &tag_and_after[tag_end + 1..];
        }
    }

    stripped.push_str(rest);
    stripped
}

fn decode_html_entities(value: &str) -> String {
    value
        .replace("&Aring;", "Å")
        .replace("&aring;", "å")
        .replace("&Auml;", "Ä")
        .replace("&auml;", "ä")
        .replace("&Ouml;", "Ö")
        .replace("&ouml;", "ö")
        .replace("&Eacute;", "É")
        .replace("&eacute;", "é")
        .replace("&amp;", "&")
        .replace("&nbsp;", " ")
        .replace("&#8203;", "")
        .replace("&quot;", "\"")
}

fn parse_swedish_weekday(value: &str) -> Option<Weekday> {
    let value = value.trim().to_uppercase();

    if value.starts_with("MÅNDAG") {
        Some(Weekday::Monday)
    } else if value.starts_with("TISDAG") {
        Some(Weekday::Tuesday)
    } else if value.starts_with("ONSDAG") {
        Some(Weekday::Wednesday)
    } else if value.starts_with("TORSDAG") {
        Some(Weekday::Thursday)
    } else if value.starts_with("FREDAG") {
        Some(Weekday::Friday)
    } else {
        None
    }
}

fn is_non_day_section(line: &str) -> bool {
    let normalized = line.trim().to_uppercase();

    normalized.starts_with("DAGENS LUNCH")
        || normalized.starts_with("ALLTID HOS SVINN")
        || normalized.starts_with("\"FÖRST TILL KVARN\"")
        || normalized.starts_with("PRIS:")
}

fn is_dish_line(line: &str) -> bool {
    if parse_swedish_weekday(line).is_some() || is_non_day_section(line) {
        return false;
    }

    let normalized = line.trim().to_uppercase();

    !normalized.starts_with("SEN TAR VI HELG")
}

fn parse_global_price(body: &str) -> Option<Price> {
    visible_text_lines(body).into_iter().find_map(|line| {
        parse_price_after(&line, "Pris ").or_else(|| parse_price_after(&line, "PRIS:"))
    })
}

fn parse_price_after(line: &str, marker: &str) -> Option<Price> {
    let marker_start = line.find(marker)?;
    let after_marker = line[marker_start + marker.len()..].trim();
    let amount = after_marker
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>()
        .parse::<u32>()
        .ok()?;

    Some(Price {
        amount,
        currency: Currency::Sek,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const KVM_MENU: &str = r#"
        <h5>Lunchmeny</h5>
        <p>11:00 - 14:00</p>
        <p>Pris 119:-</p>
        <p><b>VECKA 21, 2026</b></p>
        <div class="meny">
            <strong>Måndag</strong><br>
            Kalvfärsbiff<br>
            saftig och kryddig med lökfrästa ärtor, skysås, krämig potatispuré samt rårörda lingon.<br>
            Vegetarisk ärtfärsbiff<br>
            saftig och kryddig saftig och kryddig med lökfrästa ärtor,<i style="opacity: 0.1;color: #eee;">pogre</i><br>
            skysås, krämig potatispuré samt rårörda lingon.<i style="opacity: 0.1;color: #eee;">f</i><br>
            <br>
            <strong>Onsdag</strong><br />
            Saltimbocca på fläskkött fylld med lufttorkad skinka, serveras med krämig parmesanpolenta, balasamicosky samt toppas med syrlig äppel och ruccolasallad<br />
            Kikärts falafel<br />
            serveras med krämig parmesanpolenta, balasamicosky samt toppas med syrlig äppel och ruccolasallad<br />
            <br />
            <strong>Torsdag</strong><br />
            Frasig kycklingschnitzel<br />
            Serveras med romescosås, grillad paprika, zucchinisallad, picklad rödlök och krispig örtrostad potatis.<br />
            Frasig majs och morotsbiff<br />
            rostad broccoli, timjanssky, dragonsmör samt picklad rödlök och krispig örtrostad potatis.<br />
            <br />
            <strong>Dagens Lunch<br />Serveras kl. 11. 00–14. 00. Pris: 119 kr</strong>
        </div>
    "#;

    #[test]
    fn parses_wednesday_lunch_items() {
        let lunch = parse_lunch(KVM_MENU, Weekday::Wednesday).unwrap();

        assert_eq!(
            lunch,
            LunchState::Available {
                weekday: Weekday::Wednesday,
                items: vec![
                    LunchItem {
                        description: "Saltimbocca på fläskkött fylld med lufttorkad skinka, serveras med krämig parmesanpolenta, balasamicosky samt toppas med syrlig äppel och ruccolasallad".to_string(),
                        price: Some(Price {
                            amount: 119,
                            currency: Currency::Sek,
                        }),
                    },
                    LunchItem {
                        description: "Kikärts falafel serveras med krämig parmesanpolenta, balasamicosky samt toppas med syrlig äppel och ruccolasallad".to_string(),
                        price: Some(Price {
                            amount: 119,
                            currency: Currency::Sek,
                        }),
                    },
                ],
                notes: Vec::new(),
            }
        );
    }

    #[test]
    fn returns_weekend_no_lunch() {
        assert_eq!(
            parse_lunch(KVM_MENU, Weekday::Saturday),
            Ok(LunchState::NoLunchToday {
                weekday: Weekday::Saturday,
                reason: NoLunchReason::Weekend,
            })
        );
    }

    #[test]
    fn returns_missing_day_when_weekday_is_absent() {
        assert_eq!(
            parse_lunch(KVM_MENU, Weekday::Friday),
            Ok(LunchState::NoLunchToday {
                weekday: Weekday::Friday,
                reason: NoLunchReason::MissingDay,
            })
        );
    }
}
