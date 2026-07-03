use crate::date::Weekday;
use crate::domain::{FailureStage, LunchState, RestaurantLunch, SourceError};
use crate::restaurants::{self, RestaurantSource};

const MAX_RESTAURANTS: usize = 25;
const MAX_ITEMS_PER_RESTAURANT: usize = 20;
const MAX_ITEM_DESCRIPTION_CHARS: usize = 500;

pub fn load_todays_lunches() -> (Weekday, Vec<RestaurantLunch>) {
    let weekday = Weekday::today_utc();
    let lunches = load_lunches_for_weekday(weekday);

    (weekday, lunches)
}

fn load_lunches_for_weekday(weekday: Weekday) -> Vec<RestaurantLunch> {
    load_lunches_from_sources(weekday, restaurants::all_sources())
}

fn load_lunches_from_sources(
    weekday: Weekday,
    sources: Vec<Box<dyn RestaurantSource>>,
) -> Vec<RestaurantLunch> {
    let source_count = sources.len();
    let mut lunches = Vec::with_capacity(source_count.min(MAX_RESTAURANTS));

    for (index, source) in sources.into_iter().enumerate() {
        if index >= MAX_RESTAURANTS {
            lunches.push(RestaurantLunch {
                meta: source.meta(),
                state: size_guard_error("restaurant list", source_count, MAX_RESTAURANTS),
            });
            break;
        }

        lunches.push(RestaurantLunch {
            meta: source.meta(),
            state: guard_lunch_state(source.lunch_for(weekday)),
        });
    }

    lunches
}

fn guard_lunch_state(state: LunchState) -> LunchState {
    let LunchState::Available {
        weekday,
        items,
        notes,
    } = state
    else {
        return state;
    };

    if items.len() > MAX_ITEMS_PER_RESTAURANT {
        return size_guard_error("menu item list", items.len(), MAX_ITEMS_PER_RESTAURANT);
    }

    if let Some(actual) = items
        .iter()
        .map(|item| item.description.chars().count())
        .find(|actual| *actual > MAX_ITEM_DESCRIPTION_CHARS)
    {
        return size_guard_error("menu item description", actual, MAX_ITEM_DESCRIPTION_CHARS);
    }

    LunchState::Available {
        weekday,
        items,
        notes,
    }
}

fn size_guard_error(field: &'static str, actual: usize, limit: usize) -> LunchState {
    LunchState::Unavailable {
        stage: FailureStage::Normalize,
        error: SourceError::OutputTooLarge {
            field,
            actual,
            limit,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{LunchItem, RestaurantId, RestaurantMeta, SourceKind};

    #[test]
    fn marks_oversized_menu_item_lists_unavailable() {
        let state = LunchState::Available {
            weekday: Weekday::Wednesday,
            items: repeated_items(MAX_ITEMS_PER_RESTAURANT + 1),
            notes: Vec::new(),
        };

        assert_eq!(
            guard_lunch_state(state),
            size_guard_error(
                "menu item list",
                MAX_ITEMS_PER_RESTAURANT + 1,
                MAX_ITEMS_PER_RESTAURANT
            )
        );
    }

    #[test]
    fn marks_oversized_menu_item_descriptions_unavailable() {
        let state = LunchState::Available {
            weekday: Weekday::Wednesday,
            items: vec![LunchItem {
                description: "x".repeat(MAX_ITEM_DESCRIPTION_CHARS + 1),
            }],
            notes: Vec::new(),
        };

        assert_eq!(
            guard_lunch_state(state),
            size_guard_error(
                "menu item description",
                MAX_ITEM_DESCRIPTION_CHARS + 1,
                MAX_ITEM_DESCRIPTION_CHARS
            )
        );
    }

    #[test]
    fn keeps_other_restaurants_when_restaurant_list_is_oversized() {
        let sources = (0..=MAX_RESTAURANTS)
            .map(|_| Box::new(StubSource) as Box<dyn RestaurantSource>)
            .collect::<Vec<_>>();

        let lunches = load_lunches_from_sources(Weekday::Wednesday, sources);

        assert_eq!(lunches.len(), MAX_RESTAURANTS + 1);
        assert!(matches!(lunches[0].state, LunchState::Available { .. }));
        assert_eq!(
            lunches.last().unwrap().state,
            size_guard_error("restaurant list", MAX_RESTAURANTS + 1, MAX_RESTAURANTS)
        );
    }

    fn repeated_items(count: usize) -> Vec<LunchItem> {
        (0..count)
            .map(|_| LunchItem {
                description: "lunch".to_string(),
            })
            .collect()
    }

    struct StubSource;

    impl RestaurantSource for StubSource {
        fn meta(&self) -> RestaurantMeta {
            RestaurantMeta {
                id: RestaurantId::Gabys,
                display_name: "Stub",
                source_url: "https://example.com",
                source_kind: SourceKind::HtmlWeekdayText,
            }
        }

        fn lunch_for(&self, weekday: Weekday) -> LunchState {
            LunchState::Available {
                weekday,
                items: repeated_items(1),
                notes: Vec::new(),
            }
        }
    }
}
