use crate::date::{Date, Weekday};
use crate::domain::{
    FailureStage, LunchItem, LunchState, NoLunchReason, RestaurantId, RestaurantMeta, SourceError,
    SourceKind,
};
use crate::restaurants::{
    RestaurantSource,
    utils::{fetch_body, normalize_text, parse_swedish_weekday, visible_text_lines},
};
use scraper::{ElementRef, Html, Selector};

pub struct Vallagata;

impl RestaurantSource for Vallagata {
    fn meta(&self) -> RestaurantMeta {
        RestaurantMeta {
            id: RestaurantId::Vallagat,
            display_name: "Vällagat",
            source_url: "https://www.vallagat.se/",
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
    let mut lunch = parse_daily_lunch(body, date)?;
    if let LunchState::Available { items, .. } = &mut lunch
        && let Some(buffet) = parse_buffet(body)
    {
        items.push(buffet);
    }
    Ok(lunch)
}

fn parse_buffet(body: &str) -> Option<LunchItem> {
    let document = Html::parse_document(body);
    let heading_selector = Selector::parse("h2").expect("valid heading selector");
    let description_selector = Selector::parse("h3, p").expect("valid description selector");
    let heading = document.select(&heading_selector).find(|heading| {
        normalize_text(&heading.text().collect::<Vec<_>>().join(" "))
            .eq_ignore_ascii_case("Pick & Weigh Buffet")
    })?;
    // Wix wraps each text element in its own div within the buffet card.
    let card = ElementRef::wrap(heading.parent()?.parent()?)?;
    let description = card
        .select(&description_selector)
        .map(|element| element.text().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join(". ");
    let description = normalize_text(&description);
    if description.is_empty() {
        return None;
    }
    Some(LunchItem {
        description: format!("Pick & Weigh: {description}"),
    })
}

fn parse_daily_lunch(body: &str, date: Date) -> Result<LunchState, SourceError> {
    if matches!(date.weekday, Weekday::Saturday | Weekday::Sunday) {
        return Ok(LunchState::NoLunchToday {
            weekday: date.weekday,
            reason: NoLunchReason::Weekend,
        });
    }

    if let Some((_, script)) = body.split_once("id=\"wix-warmup-data\"") {
        let json = script
            .split_once('>')
            .and_then(|(_, content)| content.split_once("</script>"))
            .map(|(json, _)| json)
            .ok_or(SourceError::MissingStructuredData)?;
        return parse_wix_menu(json, date);
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

fn parse_wix_menu(json: &str, date: Date) -> Result<LunchState, SourceError> {
    let warmup: serde_json::Value =
        serde_json::from_str(json).map_err(|error| SourceError::InvalidJson(error.to_string()))?;
    let records = warmup
        .pointer("/appsWarmupData/dataBinding/dataStore/recordsByCollectionId/Import1")
        .and_then(serde_json::Value::as_object)
        .ok_or(SourceError::MissingExpectedElement("menu records"))?;
    let record = |category: &str| {
        records
            .values()
            .find(|record| record["newField2"].as_str() == Some(category))
    };

    // The published week is stored in the Monday field, regardless of today's day.
    let week = record("VECKA")
        .and_then(|record| record["mandag"].as_str())
        .and_then(|text| text.trim().strip_prefix("VECKA "))
        .and_then(|week| week.trim().parse::<u8>().ok())
        .filter(|week| (1..=53).contains(week))
        .ok_or(SourceError::MissingExpectedElement("menu week"))?;
    if week != date.iso_week {
        return Ok(LunchState::NoLunchToday {
            weekday: date.weekday,
            reason: NoLunchReason::Closed,
        });
    }

    let day = match date.weekday {
        Weekday::Monday => "mandag",
        Weekday::Tuesday => "tisdag",
        Weekday::Wednesday => "onsdag",
        Weekday::Thursday => "torsdag",
        Weekday::Friday => "fredag",
        Weekday::Saturday | Weekday::Sunday => unreachable!("weekends handled before parsing"),
    };
    let mut items = Vec::new();
    for category in ["GÅRDEN", "FÅNGST", "GRÖNA", "VÄRLDEN"] {
        let dish = record(category)
            .and_then(|record| record[day].as_str())
            .ok_or(SourceError::MissingExpectedElement("weekday dish"))?
            .trim();
        if dish.is_empty() {
            continue;
        }
        let sides = record(&format!("{category} TILLBEHÖR"))
            .and_then(|record| record[day].as_str())
            .ok_or(SourceError::MissingExpectedElement("dish accompaniments"))?
            .trim();
        // Preserve the site's casing, including dietary labels such as G-fri.
        let description = if sides.is_empty() {
            dish.to_string()
        } else {
            format!("{dish} | {sides}")
        };
        items.push(LunchItem { description });
    }

    if items.is_empty() {
        Ok(LunchState::NoLunchToday {
            weekday: date.weekday,
            reason: NoLunchReason::MissingDay,
        })
    } else {
        Ok(LunchState::Available {
            weekday: date.weekday,
            items,
            notes: Vec::new(),
        })
    }
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
            character.is_whitespace() || matches!(character, '\'' | '"' | '’' | '‘')
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
        "GÅRDEN:" => "GÅRDEN",
        "FÅNGST:" => "FÅNGST",
        "GRÖNA:" => "GRÖNA",
        "VÄRLDEN:" => "VÄRLDEN",
        _ => return None,
    };

    Some(category.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CURRENT_MENU: &str = include_str!("fixtures/vallagat.html");

    #[test]
    fn includes_buffet_on_available_weekdays_only() {
        let body = format!(
            r#"{CURRENT_MENU}
            <div>
                <div><h2>Pick &amp; Weigh Buffet</h2></div>
                <div><h3>Plocka själv – betala efter vikt</h3></div>
                <div><p>Salladsbuffé med grönsaker, dressingar och proteiner</p></div>
            </div>
            <div><p>Unrelated catering information</p></div>
            <script>console.log("not a dish");</script>
        "#
        );

        for weekday in [
            Weekday::Monday,
            Weekday::Tuesday,
            Weekday::Wednesday,
            Weekday::Thursday,
            Weekday::Friday,
        ] {
            let LunchState::Available { items, .. } = parse_lunch(
                &body,
                Date {
                    iso_week: 43,
                    ..date(weekday)
                },
            )
            .unwrap() else {
                panic!("expected lunch for {weekday}");
            };
            assert_eq!(
                items.last().unwrap().description,
                "Pick & Weigh: Plocka själv – betala efter vikt. Salladsbuffé med grönsaker, dressingar och proteiner"
            );
            assert_eq!(
                items
                    .iter()
                    .filter(|item| item.description.starts_with("Pick & Weigh:"))
                    .count(),
                1
            );
        }
        for (weekday, reason) in [
            (Weekday::Monday, NoLunchReason::Closed),
            (Weekday::Saturday, NoLunchReason::Weekend),
        ] {
            assert_eq!(
                parse_lunch(&body, date(weekday)),
                Ok(LunchState::NoLunchToday { weekday, reason })
            );
        }
    }

    #[test]
    fn parses_each_day_from_homepage_menu_records() {
        for (weekday, dish, sides, count) in [
            (
                Weekday::Monday,
                "Köttfärslimpa",
                "svampsås, timjansrostade morötter, pressgurka och persiljepotatis - G-fri",
                4,
            ),
            (
                Weekday::Tuesday,
                "Fläskschnitzel",
                "café de Paris-sås, rödvinssås, haricots verts och rostad potatis - L-fri",
                4,
            ),
            (
                Weekday::Wednesday,
                "Boeuf bourguignon",
                "champinjoner, sidfläsk, inlagd lök, morot och potatispuré - G-fri",
                4,
            ),
            (
                Weekday::Thursday,
                "Örtstekt secreto",
                "sås på rotselleri och äpple, hyvlad fänkål och potatispuré - G-fri",
                4,
            ),
            (
                Weekday::Friday,
                "Kalv tri tip",
                "pommes, sauce béarnaise, bordelaisesås och persiljesallad med silverlök - G-fri, L-fri",
                3,
            ),
        ] {
            let lunch = parse_lunch(
                CURRENT_MENU,
                Date {
                    iso_week: 43,
                    ..date(weekday)
                },
            )
            .unwrap();
            let LunchState::Available { items, .. } = lunch else {
                panic!("expected an available lunch for {weekday}");
            };
            assert_eq!(items.len(), count);
            assert_eq!(items[0].description, format!("{dish} | {sides}"));
            assert!(items[1].description.starts_with(match weekday {
                Weekday::Monday => "Pocherad lubb | ",
                Weekday::Tuesday => "Bakad havsöring | ",
                Weekday::Wednesday => "Friterad vittling | ",
                Weekday::Thursday => "Kryddbakad fångst | ",
                Weekday::Friday => "Havets Wallenbergare | ",
                _ => unreachable!(),
            }));
        }
    }

    #[test]
    fn rejects_stale_homepage_menu_and_skips_weekends() {
        for (weekday, reason) in [
            (Weekday::Monday, NoLunchReason::Closed),
            (Weekday::Saturday, NoLunchReason::Weekend),
        ] {
            assert_eq!(
                parse_lunch(CURRENT_MENU, date(weekday)),
                Ok(LunchState::NoLunchToday { weekday, reason })
            );
        }
    }

    #[test]
    fn reports_missing_homepage_menu_records() {
        assert_eq!(
            parse_lunch(
                r#"<script id="wix-warmup-data">{}</script>"#,
                date(Weekday::Monday),
            ),
            Err(SourceError::MissingExpectedElement("menu records"))
        );
    }

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
    fn parses_four_dishes_from_current_menu_layout() {
        let body = r#"
            <section>
              <h2>MÅNDAG vecka 34</h2>
              <p>Gården:</p>
              <p>Curry-wurst | äpple | picklad silverlök | krämig potatissallad | pankoströssel</p>
              <p>Fångst:</p>
              <p>Dillbakad kolja | brynt smör | rödbetor | kapris | pepparrot | kokt potatis</p>
              <p>Gröna:</p>
              <p>Bakade betor | getost | granatäpple | valnötssallad | rotsakskräm</p>
              <p>Världen:</p>
              <p>Krämig pasta | fläskkött | grönpeppar | riven lagrad ost | persilja | 98 kr MÅNDAGSPRIS!</p>
            </section>
        "#;
        let lunch = parse_lunch(
            body,
            Date {
                iso_year: 2026,
                iso_week: 34,
                weekday: Weekday::Monday,
            },
        )
        .unwrap();

        let LunchState::Available { items, .. } = lunch else {
            panic!("expected an available lunch");
        };

        assert_eq!(items.len(), 4);
        assert_eq!(
            items[0].description,
            "Curry-wurst | Äpple | Picklad silverlök | Krämig potatissallad | Pankoströssel"
        );
        assert_eq!(
            items[3].description,
            "Krämig pasta | Fläskkött | Grönpeppar | Riven lagrad ost | Persilja | 98 kr måndagspris!"
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
