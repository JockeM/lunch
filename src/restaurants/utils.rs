use crate::date::Weekday;
use crate::domain::SourceError;

pub(super) fn fetch_body(url: &str) -> Result<String, SourceError> {
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

pub(super) fn visible_text_lines(body: &str) -> Vec<String> {
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

pub(super) fn normalize_text(value: &str) -> String {
    value
        .replace('\u{200b}', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

pub(super) fn parse_swedish_weekday(value: &str) -> Option<Weekday> {
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

fn push_text_line(lines: &mut Vec<String>, text: &mut String) {
    let line = normalize_text(&decode_html_entities(text));

    if !line.is_empty() {
        lines.push(line);
    }

    text.clear();
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
        .replace("&#8217;", "’")
        .replace("&#039;", "'")
        .replace("&#x27;", "'")
        .replace("&quot;", "\"")
}
