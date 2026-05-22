use crate::date::Weekday;
use crate::domain::{
    FailureStage, LunchItem, LunchState, NoLunchReason, RestaurantId, RestaurantMeta, SourceError,
    SourceKind,
};
use crate::restaurants::{
    RestaurantSource,
    utils::{fetch_body, normalize_text, parse_swedish_weekday, visible_text_lines},
};

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

pub fn parse_lunch(body: &str, weekday: Weekday) -> Result<LunchState, SourceError> {
    if matches!(weekday, Weekday::Saturday | Weekday::Sunday) {
        return Ok(LunchState::NoLunchToday {
            weekday,
            reason: NoLunchReason::Weekend,
        });
    }

    let menu_lines = menu_lines(body)?;
    let day_lines = find_weekday_lines(&menu_lines, weekday);

    if day_lines.is_empty() {
        return Ok(LunchState::NoLunchToday {
            weekday,
            reason: NoLunchReason::MissingDay,
        });
    }

    let items = parse_items(day_lines);

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
        .map_or(lines.len(), |position| day_start + 1 + position);

    lines[day_start + 1..day_end].to_vec()
}

fn parse_items(lines: Vec<String>) -> Vec<LunchItem> {
    let lines = lines
        .into_iter()
        .filter(|line| is_dish_line(line))
        .collect::<Vec<_>>();

    match lines.as_slice() {
        [] => Vec::new(),
        [single] => vec![lunch_item(single)],
        [first, second] => vec![lunch_item(first), lunch_item(second)],
        lines => split_two_items(lines)
            .into_iter()
            .map(|description| lunch_item(&description))
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

fn lunch_item(description: &str) -> LunchItem {
    LunchItem {
        description: join_sentence(description),
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
        .is_some_and(char::is_uppercase)
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
                    },
                    LunchItem {
                        description: "Kikärts falafel serveras med krämig parmesanpolenta, balasamicosky samt toppas med syrlig äppel och ruccolasallad".to_string(),
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
