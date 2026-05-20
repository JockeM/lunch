use crate::date::Weekday;
use crate::domain::RestaurantLunch;
use crate::restaurants;

pub fn load_todays_lunches() -> (Weekday, Vec<RestaurantLunch>) {
    let weekday = Weekday::today_utc();
    let lunches = load_lunches_for_weekday(weekday);

    (weekday, lunches)
}

fn load_lunches_for_weekday(weekday: Weekday) -> Vec<RestaurantLunch> {
    restaurants::all_sources()
        .into_iter()
        .map(|source| RestaurantLunch {
            meta: source.meta(),
            state: source.lunch_for(weekday),
        })
        .collect()
}
