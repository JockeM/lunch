use crate::date::Weekday;
use crate::domain::{LunchState, RestaurantId, RestaurantMeta, SourceError, SourceKind};
use crate::restaurants::RestaurantSource;

pub struct JinxEmpire;

impl RestaurantSource for JinxEmpire {
    fn meta(&self) -> RestaurantMeta {
        RestaurantMeta {
            id: RestaurantId::JinxEmpire,
            display_name: "Jinx Empire",
            source_url: "https://www.jinxempire.com/#menu",
            source_kind: SourceKind::JsonLdMenu,
        }
    }

    fn lunch_for(&self, _weekday: Weekday) -> Result<LunchState, SourceError> {
        Err(SourceError::NotImplemented)
    }
}
