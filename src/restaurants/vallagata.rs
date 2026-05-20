use crate::date::Weekday;
use crate::domain::{
    FailureStage, LunchState, RestaurantId, RestaurantMeta, SourceError, SourceKind,
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

    fn lunch_for(&self, _weekday: Weekday) -> LunchState {
        LunchState::Unavailable {
            stage: FailureStage::Parse,
            error: SourceError::NotImplemented,
        }
    }
}
