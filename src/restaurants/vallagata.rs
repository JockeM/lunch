use crate::date::Weekday;
use crate::domain::{LunchState, RestaurantId, RestaurantMeta, SourceError, SourceKind};
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

    fn lunch_for(&self, _weekday: Weekday) -> Result<LunchState, SourceError> {
        Err(SourceError::NotImplemented)
    }
}
