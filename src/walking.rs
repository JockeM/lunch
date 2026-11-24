use crate::domain::RestaurantId;

pub const OFFICE_ADDRESS: &str = "Johan Willins gata 2-8, 416 64 Göteborg";

pub struct OfficeWalk {
    pub destination: &'static str,
    pub distance_meters: u16,
    pub minutes: u16,
}

impl OfficeWalk {
    pub fn label(&self) -> String {
        format!("🚶 {} m · ~{} min", self.distance_meters, self.minutes)
    }

    pub fn directions_url(&self) -> String {
        let mut url =
            reqwest::Url::parse("https://www.google.com/maps/dir/").expect("valid Google Maps URL");
        url.query_pairs_mut().extend_pairs([
            ("api", "1"),
            ("origin", OFFICE_ADDRESS),
            ("destination", self.destination),
            ("travelmode", "walking"),
        ]);
        url.into()
    }
}

pub fn walk_from_office(restaurant: RestaurantId) -> OfficeWalk {
    let (destination, distance_meters, minutes) = match restaurant {
        RestaurantId::Vallagat => ("Johan Willins gata 6, Göteborg", 250, 4),
        RestaurantId::JinxEmpire => ("Folkungagatan 20, Göteborg", 450, 6),
        RestaurantId::Svinn => ("Norra Ågatan 6, Göteborg", 400, 5),
        RestaurantId::Gabys => ("Drakegatan 10, Göteborg", 650, 9),
    };
    OfficeWalk {
        destination,
        distance_meters,
        minutes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_the_configured_distance_and_walking_time() {
        let walk = OfficeWalk {
            destination: "Folkungagatan 20, Göteborg",
            distance_meters: 450,
            minutes: 10,
        };
        assert_eq!(walk.label(), "🚶 450 m · ~10 min");
    }

    #[test]
    fn directions_use_the_office_and_each_restaurants_address() {
        for (restaurant, destination) in [
            (RestaurantId::Vallagat, "Johan Willins gata 6, Göteborg"),
            (RestaurantId::JinxEmpire, "Folkungagatan 20, Göteborg"),
            (RestaurantId::Svinn, "Norra Ågatan 6, Göteborg"),
            (RestaurantId::Gabys, "Drakegatan 10, Göteborg"),
        ] {
            let url = reqwest::Url::parse(&walk_from_office(restaurant).directions_url()).unwrap();
            let params = url
                .query_pairs()
                .collect::<std::collections::HashMap<_, _>>();
            assert_eq!(params["api"], "1");
            assert_eq!(params["origin"], OFFICE_ADDRESS);
            assert_eq!(params["destination"], destination);
            assert_eq!(params["travelmode"], "walking");
        }
    }
}
