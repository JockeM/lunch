mod gabys;
mod jinx;
mod svinn;
mod utils;
mod vallagata;

use crate::date::Date;
use crate::domain::{LunchState, RestaurantId, RestaurantMeta, SourceKind};

pub trait RestaurantSource {
    fn meta(&self) -> RestaurantMeta;

    fn lunch_for(&self, date: Date) -> LunchState;
}

pub fn all_sources() -> Vec<Box<dyn RestaurantSource>> {
    vec![
        Box::new(vallagata::Vallagata),
        Box::new(jinx::JinxEmpire),
        Box::new(svinn::Svinn),
        Box::new(gabys::Gabys),
        website_only(
            RestaurantId::Evolushi,
            "Evolushi Stampgatan",
            "https://thatsup.se/goteborg/restaurang/evolushi-stampgatan/",
        ),
        website_only(
            RestaurantId::BirriaSeoul,
            "Birria Seoul",
            "https://thatsup.se/goteborg/restaurang/birria-seoul/",
        ),
        website_only(
            RestaurantId::UlleviThaikok,
            "Ullevi Thaikök",
            "https://thatsup.se/goteborg/restaurang/ullevi-thaikok/",
        ),
    ]
}

fn website_only(
    id: RestaurantId,
    display_name: &'static str,
    source_url: &'static str,
) -> Box<dyn RestaurantSource> {
    Box::new(WebsiteOnly(RestaurantMeta {
        id,
        display_name,
        source_url,
        source_kind: SourceKind::WebsiteOnly,
    }))
}

struct WebsiteOnly(RestaurantMeta);

impl RestaurantSource for WebsiteOnly {
    fn meta(&self) -> RestaurantMeta {
        self.0
    }

    fn lunch_for(&self, _: Date) -> LunchState {
        LunchState::ListedOnly
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn lists_website_only_restaurants_without_daily_menu_content() {
        use crate::date::Weekday;
        use crate::domain::RestaurantLunch;
        use crate::render::{render_day, render_slack_payload};

        let sources = all_sources();
        let date = Date {
            iso_year: 2026,
            iso_week: 21,
            weekday: Weekday::Wednesday,
        };
        for (id, name, url) in [
            (
                RestaurantId::Evolushi,
                "Evolushi Stampgatan",
                "https://thatsup.se/goteborg/restaurang/evolushi-stampgatan/",
            ),
            (
                RestaurantId::BirriaSeoul,
                "Birria Seoul",
                "https://thatsup.se/goteborg/restaurang/birria-seoul/",
            ),
            (
                RestaurantId::UlleviThaikok,
                "Ullevi Thaikök",
                "https://thatsup.se/goteborg/restaurang/ullevi-thaikok/",
            ),
        ] {
            let source = sources
                .iter()
                .find(|source| source.meta().id == id)
                .unwrap();
            let lunches = [RestaurantLunch {
                meta: source.meta(),
                state: source.lunch_for(date),
            }];

            assert_eq!(
                render_day(date.weekday, &lunches),
                format!("Today (Wednesday)\n\n{name}\nWebsite: {url}")
            );
            let payload: serde_json::Value =
                serde_json::from_str(&render_slack_payload(date.weekday, &lunches)).unwrap();
            assert_eq!(
                payload["blocks"][2]["text"]["text"],
                format!("*<{url}|{name}>*")
            );
        }
    }

    #[test]
    fn registered_sources_have_unique_ids() {
        let mut ids = HashSet::new();

        for source in all_sources() {
            let id = source.meta().id;

            assert!(ids.insert(id), "duplicate restaurant id {id:?}");
        }
    }
}
