mod jinx;
mod vallagata;

use crate::date::Weekday;
use crate::domain::{LunchState, RestaurantMeta};

pub trait RestaurantSource {
    fn meta(&self) -> RestaurantMeta;

    fn lunch_for(&self, weekday: Weekday) -> LunchState;
}

pub fn all_sources() -> Vec<Box<dyn RestaurantSource>> {
    vec![Box::new(jinx::JinxEmpire), Box::new(vallagata::Vallagata)]
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn registered_sources_have_unique_ids() {
        let mut ids = HashSet::new();

        for source in all_sources() {
            let id = source.meta().id;

            assert!(ids.insert(id), "duplicate restaurant id {id:?}");
        }
    }
}
