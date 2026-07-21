use crate::date::{Date, Weekday};
use crate::domain::{
    FailureStage, LunchItem, LunchState, NoLunchReason, RestaurantId, RestaurantMeta, SourceError,
    SourceKind,
};
use crate::restaurants::{
    RestaurantSource,
    utils::{fetch_body, parse_swedish_weekday, visible_text_lines},
};

pub struct Vallagata;

impl RestaurantSource for Vallagata {
    fn meta(&self) -> RestaurantMeta {
        RestaurantMeta {
            id: RestaurantId::Vallagat,
            display_name: "Vällagat",
            source_url: "https://www.vallagat.se/lunchmeny",
            source_kind: SourceKind::HtmlWeekdayText,
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
    if matches!(date.weekday, Weekday::Saturday | Weekday::Sunday) {
        return Ok(LunchState::NoLunchToday {
            weekday: date.weekday,
            reason: NoLunchReason::Weekend,
        });
    }

    if menu_week(body).is_some_and(|menu_week| menu_week != date.iso_week) {
        return Ok(LunchState::NoLunchToday {
            weekday: date.weekday,
            reason: NoLunchReason::Closed,
        });
    }

    let section_lines = find_weekday_section_lines(body, date.weekday)?;
    let items = parse_items(section_lines);

    if items.is_empty() {
        return Ok(LunchState::NoLunchToday {
            weekday: date.weekday,
            reason: NoLunchReason::MissingDay,
        });
    }

    Ok(LunchState::Available {
        weekday: date.weekday,
        items,
        notes: Vec::new(),
    })
}

fn menu_week(body: &str) -> Option<u8> {
    visible_text_lines(body).into_iter().find_map(|line| {
        parse_swedish_weekday(&line)?;
        let line = line.to_lowercase();
        let (_, week_text) = line.split_once("vecka")?;
        let digits = week_text
            .trim_start()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>();
        let week = digits.parse::<u8>().ok()?;

        (1..=53).contains(&week).then_some(week)
    })
}

fn parse_items(lines: Vec<String>) -> Vec<LunchItem> {
    lines
        .into_iter()
        .filter(|line| is_dish_line(line))
        .map(|description| lunch_item(&description))
        .collect()
}

fn lunch_item(description: &str) -> LunchItem {
    LunchItem {
        description: format_dish_description(description),
    }
}

fn format_dish_description(description: &str) -> String {
    description
        .split('|')
        .filter_map(|part| {
            let part = clean_dish_part(part);

            if part.is_empty() {
                None
            } else {
                Some(capitalize_first_word(&part))
            }
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn clean_dish_part(part: &str) -> String {
    part.replace("’’", " ")
        .replace("‘‘", " ")
        .replace("\"\"", " ")
        .trim_matches(|character: char| {
            character.is_whitespace()
                || matches!(character, '\'' | '"' | '’' | '‘')
        })
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn capitalize_first_word(value: &str) -> String {
    let Some(first_character) = value.chars().next() else {
        return String::new();
    };
    let first_character_len = first_character.len_utf8();
    let first = first_character.to_uppercase().collect::<String>();
    let rest = value[first_character_len..].to_lowercase();

    apply_known_casing(&format!("{first}{rest}"))
}

fn apply_known_casing(value: &str) -> String {
    value
        .split_whitespace()
        .map(|word| match word {
            "bellman" => "Bellman".to_string(),
            "ceasar" | "caesar" => "Caesar".to_string(),
            _ => word.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn find_weekday_section_lines(body: &str, weekday: Weekday) -> Result<Vec<String>, SourceError> {
    let sections = body.split("<section");

    for section in sections {
        let section_body = section
            .find('>')
            .map_or(section, |tag_end| &section[tag_end + 1..]);
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
        let lunch = parse_lunch(WIX_MENU, date(Weekday::Wednesday)).unwrap();

        assert_eq!(
            lunch,
            LunchState::Available {
                weekday: Weekday::Wednesday,
                items: vec![
                    LunchItem {
                        description: "Bulgogi | Strimlat nötkött | Jasminris | Kålsallad"
                            .to_string(),
                    },
                    LunchItem {
                        description: "Dagens fångst | Skordalia | Tomat | Oliver".to_string(),
                    },
                    LunchItem {
                        description: "Halloumibiff | Skordalia | Tomat | Oliver".to_string(),
                    },
                    LunchItem {
                        description: "Nudelwok | Kyckling | Vetenudlar | Salladslök".to_string(),
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
        let lunch = parse_lunch(body, date(Weekday::Wednesday)).unwrap();

        assert!(matches!(lunch, LunchState::Available { .. }));
    }

    #[test]
    fn decodes_and_cleans_decorative_dish_quotes() {
        let body = r"
            <section>
              <p>Kyckling&rsquo;&rsquo;ceasar&rsquo;&rsquo; | Romansallad | Bacon</p>
              <p>&rsquo;&rsquo;pytt bellman&rsquo;&rsquo; | Chilisås | Persilja</p>
              <p>ONSDAG</p>
            </section>
        ";
        let lunch = parse_lunch(body, date(Weekday::Wednesday)).unwrap();

        assert_eq!(
            lunch,
            LunchState::Available {
                weekday: Weekday::Wednesday,
                items: vec![
                    LunchItem {
                        description: "Kyckling Caesar | Romansallad | Bacon".to_string(),
                    },
                    LunchItem {
                        description: "Pytt Bellman | Chilisås | Persilja".to_string(),
                    },
                ],
                notes: Vec::new(),
            }
        );
    }

    #[test]
    fn formats_dish_description_parts() {
        assert_eq!(
            format_dish_description("BULGOGI |strimlat nötkött |jasminris | |"),
            "Bulgogi | Strimlat nötkött | Jasminris"
        );
    }

    #[test]
    fn returns_closed_when_only_an_old_menu_is_published() {
        assert_eq!(
            parse_lunch(
                WIX_MENU,
                Date {
                    iso_year: 2026,
                    iso_week: 30,
                    weekday: Weekday::Wednesday,
                }
            ),
            Ok(LunchState::NoLunchToday {
                weekday: Weekday::Wednesday,
                reason: NoLunchReason::Closed,
            })
        );
    }
}
