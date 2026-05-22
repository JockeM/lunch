use crate::date::Weekday;
use crate::domain::{
    FailureStage, LunchItem, LunchState, NoLunchReason, RestaurantId, RestaurantMeta, SourceError,
    SourceKind,
};
use crate::restaurants::{
    RestaurantSource,
    utils::{fetch_body, normalize_text, visible_text_lines},
};

pub struct Gabys;

impl RestaurantSource for Gabys {
    fn meta(&self) -> RestaurantMeta {
        RestaurantMeta {
            id: RestaurantId::Gabys,
            display_name: "Gaby's",
            source_url: "https://jacyzhotel.com/restauranger-goteborg/gabys/#lunch",
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

pub fn parse_lunch(body: &str, weekday: Weekday) -> Result<LunchState, SourceError> {
    if matches!(weekday, Weekday::Saturday | Weekday::Sunday) {
        return Ok(LunchState::NoLunchToday {
            weekday,
            reason: NoLunchReason::Weekend,
        });
    }

    let lines = lunch_lines(body)?;
    let day_lines = find_weekday_lines(&lines, weekday);

    if day_lines.is_empty() {
        return Ok(LunchState::NoLunchToday {
            weekday,
            reason: NoLunchReason::MissingDay,
        });
    }

    let items = day_lines
        .into_iter()
        .filter(|line| is_dish_line(line))
        .map(|description| LunchItem {
            description: normalize_text(&description),
        })
        .collect::<Vec<_>>();

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

fn lunch_lines(body: &str) -> Result<Vec<String>, SourceError> {
    let lines = visible_text_lines(body);
    let menu_start = lines
        .iter()
        .position(|line| line.to_uppercase().starts_with("LUNCHMENY"))
        .ok_or(SourceError::MissingExpectedElement("LUNCHMENY"))?;
    let menu_end = lines[menu_start + 1..]
        .iter()
        .position(|line| is_after_lunch_section(line))
        .map_or(lines.len(), |position| menu_start + 1 + position);

    Ok(lines[menu_start..menu_end].to_vec())
}

fn is_after_lunch_section(line: &str) -> bool {
    line.starts_with("What’s for lunch") || line.starts_with("What's for lunch")
}

fn find_weekday_lines(lines: &[String], weekday: Weekday) -> Vec<String> {
    let Some(day_start) = lines
        .iter()
        .position(|line| parse_english_weekday(line) == Some(weekday))
    else {
        return Vec::new();
    };
    let day_end = lines[day_start + 1..]
        .iter()
        .position(|line| parse_english_weekday(line).is_some())
        .map_or(lines.len(), |position| day_start + 1 + position);

    lines[day_start + 1..day_end].to_vec()
}

fn parse_english_weekday(value: &str) -> Option<Weekday> {
    match value.trim().to_ascii_lowercase().as_str() {
        "monday" => Some(Weekday::Monday),
        "tuesday" => Some(Weekday::Tuesday),
        "wednesday" => Some(Weekday::Wednesday),
        "thursday" => Some(Weekday::Thursday),
        "friday" => Some(Weekday::Friday),
        _ => None,
    }
}

fn is_dish_line(line: &str) -> bool {
    let normalized = line.trim();

    !normalized.is_empty()
        && parse_english_weekday(normalized).is_none()
        && !normalized.to_ascii_lowercase().contains("sek / pers")
}

#[cfg(test)]
mod tests {
    use super::*;

    const GABYS_MENU: &str = r#"
        <h2>LUNCHMENY vecka 21</h2>
        <p>Just swing by, no table reservations.</p>
        <p>Lunch is served between 11:00 – 13:30 Monday to Friday</p>
        <p>Includes salad, bread and coffee.</p>
        <p>139 SEK / pers.</p>
        <h3>Salad of the week</h3>
        <p>Hoisinbakad fläsksida, kål, morot, mango</p>
        <h3>Poke Bowl – 159 SEK / pers.</h3>
        <p>Glasnudlar, lax, sojabönor, mango</p>
        <h3>Monday</h3>
        <p>Bakad fisk, potatispuré, brynt smör</p>
        <p>Gaby´s flygande Jacob på kycklingbröst, ris</p>
        <p>Svamppasta, tryffel, parmesan</p>
        <h3>Tuesday</h3>
        <p>Panko friterad kummel, kokt potatis</p>
        <p>Köttbullar, potatispuré, gräddsås</p>
        <p>Het kikärtsgryta, blomkål, ris</p>
        <h2>What’s for lunch, sweetheart?</h2>
    "#;

    #[test]
    fn parses_weekday_lunch_items() {
        let lunch = parse_lunch(GABYS_MENU, Weekday::Monday).unwrap();

        assert_eq!(
            lunch,
            LunchState::Available {
                weekday: Weekday::Monday,
                items: vec![
                    LunchItem {
                        description: "Bakad fisk, potatispuré, brynt smör".to_string(),
                    },
                    LunchItem {
                        description: "Gaby´s flygande Jacob på kycklingbröst, ris".to_string(),
                    },
                    LunchItem {
                        description: "Svamppasta, tryffel, parmesan".to_string(),
                    },
                ],
                notes: Vec::new(),
            }
        );
    }

    #[test]
    fn returns_weekend_no_lunch() {
        assert_eq!(
            parse_lunch(GABYS_MENU, Weekday::Sunday),
            Ok(LunchState::NoLunchToday {
                weekday: Weekday::Sunday,
                reason: NoLunchReason::Weekend,
            })
        );
    }

    #[test]
    fn ignores_standing_items_before_weekday_blocks() {
        let lunch = parse_lunch(GABYS_MENU, Weekday::Tuesday).unwrap();

        assert!(matches!(lunch, LunchState::Available { items, .. } if items.len() == 3));
    }

    #[test]
    fn stops_at_html_encoded_after_lunch_heading() {
        let body = r#"
            <h2>LUNCHMENY vecka 21</h2>
            <h3>Friday</h3>
            <p>Halstrad lax, kokt potatis</p>
            <h2>What&#8217;s for lunch, sweetheart?</h2>
            <p>This should not be parsed as Friday lunch.</p>
        "#;

        let lunch = parse_lunch(body, Weekday::Friday).unwrap();

        assert_eq!(
            lunch,
            LunchState::Available {
                weekday: Weekday::Friday,
                items: vec![LunchItem {
                    description: "Halstrad lax, kokt potatis".to_string(),
                }],
                notes: Vec::new(),
            }
        );
    }
}
