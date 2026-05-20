use crate::date::Weekday;
use crate::domain::{
    Currency, FailureStage, LunchItem, LunchState, NoLunchReason, Price, RestaurantId,
    RestaurantMeta, SourceError, SourceKind,
};
use crate::restaurants::RestaurantSource;

pub struct Vallagata;

impl RestaurantSource for Vallagata {
    fn meta(&self) -> RestaurantMeta {
        RestaurantMeta {
            id: RestaurantId::Vallagata,
            display_name: "Vallagata",
            source_url: "https://www.vallagat.se/lunchmeny",
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
    let section_lines = find_weekday_section_lines(body, weekday)?;
    let items = parse_items(section_lines, price);

    if items.is_empty() {
        return Ok(LunchState::NoLunchToday {
            weekday,
            reason: NoLunchReason::MissingDay,
        });
    }

    Ok(LunchState::Available {
        weekday,
        items,
        notes: Vec::new(),
    })
}

fn parse_items(lines: Vec<String>, price: Option<Price>) -> Vec<LunchItem> {
    lines
        .into_iter()
        .filter(|line| is_dish_line(line))
        .map(|description| lunch_item(description, price.clone()))
        .collect()
}

fn lunch_item(description: String, price: Option<Price>) -> LunchItem {
    LunchItem {
        description: format_dish_description(&description),
        price,
    }
}

fn format_dish_description(description: &str) -> String {
    description
        .split('|')
        .filter_map(|part| {
            let part = part.trim();

            if part.is_empty() {
                None
            } else {
                Some(capitalize_first_word(part))
            }
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn capitalize_first_word(value: &str) -> String {
    let Some(first_character) = value.chars().next() else {
        return String::new();
    };
    let first_character_len = first_character.len_utf8();
    let first = first_character.to_uppercase().collect::<String>();
    let rest = value[first_character_len..].to_lowercase();

    format!("{first}{rest}")
}

fn find_weekday_section_lines(body: &str, weekday: Weekday) -> Result<Vec<String>, SourceError> {
    let sections = body.split("<section");

    for section in sections {
        let section_body = section
            .find('>')
            .map(|tag_end| &section[tag_end + 1..])
            .unwrap_or(section);
        let lines = visible_text_lines(section_body);

        if lines
            .iter()
            .any(|line| parse_swedish_weekday(line) == Some(weekday))
        {
            return Ok(lines);
        }
    }

    if visible_text_lines(body)
        .iter()
        .any(|line| parse_swedish_weekday(line).is_some())
    {
        Ok(Vec::new())
    } else {
        Err(SourceError::MissingExpectedElement("weekday section"))
    }
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

fn is_dish_line(line: &str) -> bool {
    if parse_swedish_weekday(line).is_some() {
        return false;
    }

    let normalized = line.trim().to_uppercase();

    parse_category(&normalized).is_none()
        && !normalized.starts_with("PRIS:")
        && !normalized.starts_with("SERVERAS ")
        && !normalized.starts_with("VEGANSK MAT ")
        && !normalized.starts_with("KONTAKTA OSS ")
}

fn parse_category(line: &str) -> Option<String> {
    let normalized = line.trim().to_uppercase();
    let category = match normalized.as_str() {
        "KÖTT:" => "KÖTT",
        "FISK:" => "FISK",
        "VEG:" => "VEG",
        "STREETFOOD:" | "STREETFOOD PÅ BUFFÉ:" => "STREETFOOD",
        _ => return None,
    };

    Some(category.to_string())
}

fn parse_global_price(body: &str) -> Option<Price> {
    visible_text_lines(body).into_iter().find_map(|line| {
        let upper = line.to_uppercase();
        let price_start = upper.find("PRIS:")?;
        let after_price = line[price_start + "PRIS:".len()..].trim();
        let amount = after_price.split_whitespace().next()?.parse::<u32>().ok()?;

        Some(Price {
            amount,
            currency: Currency::Sek,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIX_MENU: &str = r#"
        <section>
          <p>LÅNGBAKAD FLÄSKKARRE| morotspuré |rostad potatis</p>
          <p>KÖTT:</p>
          <p>DAGENS FÅNGST |skaldjurssås | grönkål</p>
          <p>FISK:</p>
          <p>VEG:</p>
          <p>STREETFOOD:</p>
          <p>BAKAD MOROT |morotspuré | smörbönor</p>
          <p>PASTA RAGU| högrev| sidfläsk |tomat |95 kr</p>
          <p>MÅNDAG vecka 21</p>
        </section>
        <section>
          <p>BULGOGI |strimlat nötkött |jasminris |kålsallad</p>
          <p>KÖTT:</p>
          <p>DAGENS FÅNGST |skordalia |tomat | oliver</p>
          <p>FISK:</p>
          <p>VEG:</p>
          <p>STREETFOOD:</p>
          <p>HALLOUMIBIFF| skordalia |tomat | oliver</p>
          <p>NUDELWOK |kyckling |vetenudlar | salladslök</p>
          <p>ONSDAG</p>
        </section>
        <section>
          <p>PRIS: 132 kr - LUNCHHÄFTE - 1200 kr</p>
        </section>
    "#;

    #[test]
    fn parses_weekday_section() {
        let lunch = parse_lunch(WIX_MENU, Weekday::Wednesday).unwrap();

        assert_eq!(
            lunch,
            LunchState::Available {
                weekday: Weekday::Wednesday,
                items: vec![
                    LunchItem {
                        description: "Bulgogi | Strimlat nötkött | Jasminris | Kålsallad"
                            .to_string(),
                        price: Some(Price {
                            amount: 132,
                            currency: Currency::Sek,
                        }),
                    },
                    LunchItem {
                        description: "Dagens fångst | Skordalia | Tomat | Oliver".to_string(),
                        price: Some(Price {
                            amount: 132,
                            currency: Currency::Sek,
                        }),
                    },
                    LunchItem {
                        description: "Halloumibiff | Skordalia | Tomat | Oliver".to_string(),
                        price: Some(Price {
                            amount: 132,
                            currency: Currency::Sek,
                        }),
                    },
                    LunchItem {
                        description: "Nudelwok | Kyckling | Vetenudlar | Salladslök".to_string(),
                        price: Some(Price {
                            amount: 132,
                            currency: Currency::Sek,
                        }),
                    },
                ],
                notes: Vec::new(),
            }
        );
    }

    #[test]
    fn decodes_wix_entities() {
        let body = r#"
            <section>
              <p>DAGENS F&Aring;NGST |skaldjurss&aring;s | gr&ouml;nk&aring;l</p>
              <p>ONSDAG&nbsp;</p>
            </section>
        "#;
        let lunch = parse_lunch(body, Weekday::Wednesday).unwrap();

        assert!(matches!(lunch, LunchState::Available { .. }));
    }

    #[test]
    fn formats_dish_description_parts() {
        assert_eq!(
            format_dish_description("BULGOGI |strimlat nötkött |jasminris | |"),
            "Bulgogi | Strimlat nötkött | Jasminris"
        );
    }
}
