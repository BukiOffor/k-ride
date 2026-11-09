//! Dynamic Fare Estimation and Routing Engine
//! Uses validated OSRM road routes; routing failures cannot generate fare quotes.
//! Implements waiting-time calculation (free 5-min period + per-additional-minute rate).

use crate::vrp::dtos::rides::{
    FareLine, FareQuote, FareRange, LatLng, Money, RouteSource, WaitingPolicy,
};
use crate::vrp::models::rides::RideOption;

#[derive(Debug, Clone)]
pub struct RouteResult {
    pub points: Vec<LatLng>,
    pub distance_meters: i64,
    pub duration_seconds: i64,
    pub source: RouteSource,
}

/// Fetches a real road route; unavailable routing never produces an approximate fare.
pub async fn compute_route(
    pickup: &LatLng,
    destination: &LatLng,
) -> Result<RouteResult, dispatcher::routing::RoutingError> {
    let route = dispatcher::routing::route(
        &dispatcher::routing::primary_url(),
        [pickup.lng, pickup.lat],
        [destination.lng, destination.lat],
    )
    .await?;
    Ok(RouteResult {
        points: route
            .coordinates
            .into_iter()
            .map(|[lng, lat]| LatLng { lat, lng })
            .collect(),
        distance_meters: route.distance.round().max(1.0) as i64,
        duration_seconds: route.duration.round().max(1.0) as i64,
        source: RouteSource::Road,
    })
}

/// Calculate dynamic fare quote for a specific RideOption based on route distance and duration
pub fn calculate_fare_quote(
    option: &RideOption,
    distance_meters: i64,
    duration_seconds: i64,
) -> FareQuote {
    let km = distance_meters as f64 / 1000.0;
    let minutes = duration_seconds as f64 / 60.0;

    let distance_fare = (km * option.per_km_minor as f64).round() as i64;
    let time_fare = (minutes * option.per_minute_minor as f64).round() as i64;

    let base_fare = option.base_fare_minor + 300_000;
    let subtotal = base_fare + distance_fare + time_fare + option.booking_fee_minor;
    let mut total = subtotal.max(option.minimum_fare_minor);

    // Apply rounding if configured (e.g., 5000 minor units = 50 NGN)
    if option.rounding_minor > 0 {
        let remainder = total % option.rounding_minor;
        if remainder > 0 {
            total += option.rounding_minor - remainder;
        }
    }

    let low = (total as f64 * 0.95).round() as i64;
    let high = (total as f64 * 1.10).round() as i64;

    let breakdown = vec![
        FareLine {
            code: "base".into(),
            label: "Base Fare".into(),
            amount: Money::new(base_fare, &option.currency),
        },
        FareLine {
            code: "distance".into(),
            label: format!("{:.1} km", km),
            amount: Money::new(distance_fare, &option.currency),
        },
        FareLine {
            code: "time".into(),
            label: format!("{:.0} mins", minutes),
            amount: Money::new(time_fare, &option.currency),
        },
    ];

    FareQuote {
        total: Money::new(total, &option.currency),
        range: FareRange {
            low: Money::new(low, &option.currency),
            high: Money::new(high, &option.currency),
        },
        breakdown,
        is_estimate: true,
        waiting: WaitingPolicy {
            free_minutes: option.free_waiting_minutes,
            per_minute: Money::new(option.waiting_per_minute_minor, &option.currency),
        },
    }
}

/// Recognize the airport's full name in a selected place label. Do not match
/// generic airport labels or nearby addresses supplied as secondary text.
pub fn is_nnamdi_azikiwe_airport(place: &crate::vrp::dtos::rides::Place) -> bool {
    let normalized = place
        .name
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let padded = format!(" {normalized} ");
    padded.contains(" nnamdi azikiwe airport ")
        || padded.contains(" nnamdi azikiwe international airport ")
}

/// A fixed NGN 40,000 quote, expressed in kobo, for either airport endpoint.
/// Both immediate and scheduled bookings persist this estimate unchanged.
pub fn apply_airport_fare(
    quote: &mut FareQuote,
    pickup: &crate::vrp::dtos::rides::Place,
    destination: &crate::vrp::dtos::rides::Place,
) {
    if !is_nnamdi_azikiwe_airport(pickup) && !is_nnamdi_azikiwe_airport(destination) {
        return;
    }
    let fare = Money::new(4_000_000, "NGN");
    quote.total = fare.clone();
    quote.range = FareRange {
        low: fare.clone(),
        high: fare.clone(),
    };
    quote.breakdown = vec![FareLine {
        code: "airport_fixed".into(),
        label: "Nnamdi Azikiwe Airport fare".into(),
        amount: fare,
    }];
    quote.is_estimate = false;
    quote.waiting.per_minute = Money::new(0, "NGN");
}

/// Calculate waiting time charge:
/// Free for the first 5 minutes (or option.free_waiting_minutes).
/// Each additional started minute after the free window incurs option.waiting_per_minute_minor.
pub fn calculate_waiting_charge(
    option: &RideOption,
    arrived_at: chrono::NaiveDateTime,
    started_at: chrono::NaiveDateTime,
) -> (i64, i64, i64) {
    let waited_seconds = (started_at - arrived_at).num_seconds().max(0);
    let free_seconds = (option.free_waiting_minutes as i64) * 60;

    if waited_seconds <= free_seconds {
        return (waited_seconds, 0, 0);
    }

    let billable_seconds = waited_seconds - free_seconds;
    // Charge per additional started minute (ceil)
    let billable_minutes = (billable_seconds + 59) / 60;
    let charge_minor = billable_minutes * option.waiting_per_minute_minor;

    (waited_seconds, billable_minutes, charge_minor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn place(name: &str) -> crate::vrp::dtos::rides::Place {
        crate::vrp::dtos::rides::Place {
            id: None,
            name: name.into(),
            secondary: None,
            point: LatLng { lat: 0.0, lng: 0.0 },
            source: None,
        }
    }

    #[test]
    fn airport_names_are_normalized_and_specific() {
        for name in [
            "Nnamdi Azikiwe Airport",
            "NNAMDI AZIKIWE INTERNATIONAL AIRPORT, Abuja",
            "Nnamdi-Azikiwe  Airport (ABV)",
        ] {
            assert!(is_nnamdi_azikiwe_airport(&place(name)));
        }
        for name in [
            "Airport",
            "Murtala Muhammed International Airport",
            "Nnamdi Azikiwe University",
            "Nnamdi Azikiwe Airportway",
        ] {
            assert!(!is_nnamdi_azikiwe_airport(&place(name)));
        }
    }

    #[test]
    fn airport_fare_is_exact_in_both_directions_and_leaves_other_trips_unchanged() {
        let original = FareQuote {
            total: Money::new(120000, "NGN"),
            range: FareRange {
                low: Money::new(100000, "NGN"),
                high: Money::new(150000, "NGN"),
            },
            breakdown: vec![],
            is_estimate: true,
            waiting: WaitingPolicy {
                free_minutes: 5,
                per_minute: Money::new(3000, "NGN"),
            },
        };
        let airport = place("Nnamdi Azikiwe International Airport");
        let city = place("City Centre");
        for (pickup, destination) in [(&airport, &city), (&city, &airport), (&airport, &airport)] {
            let mut quote = original.clone();
            apply_airport_fare(&mut quote, pickup, destination);
            assert_eq!(quote.total.amount_minor, 4_000_000);
            assert_eq!(quote.total.currency, "NGN");
            assert_eq!(quote.range.low.amount_minor, 4_000_000);
            assert_eq!(quote.range.high.amount_minor, 4_000_000);
            assert_eq!(
                quote
                    .breakdown
                    .iter()
                    .map(|line| line.amount.amount_minor)
                    .sum::<i64>(),
                4_000_000
            );
            assert!(!quote.is_estimate);
            assert_eq!(quote.waiting.per_minute.amount_minor, 0);
        }
        let mut quote = original.clone();
        apply_airport_fare(&mut quote, &city, &city);
        assert_eq!(
            serde_json::to_value(quote).unwrap(),
            serde_json::to_value(original).unwrap()
        );
    }

    #[test]
    fn test_waiting_charge_within_free_window() {
        let opt = RideOption {
            id: "go".into(),
            name: "Go".into(),
            description: "".into(),
            vehicle_class: "sedan".into(),
            eligible_vehicle_classes: vec![],
            capacity: 4,
            shared: false,
            currency: "NGN".into(),
            base_fare_minor: 50000,
            per_km_minor: 15000,
            per_minute_minor: 2000,
            minimum_fare_minor: 100000,
            booking_fee_minor: 0,
            free_waiting_minutes: 5,
            waiting_per_minute_minor: 3000,
            rounding_minor: 5000,
            is_active: true,
            sort_order: 1,
            created_at: chrono::Utc::now().naive_utc(),
            updated_at: chrono::Utc::now().naive_utc(),
        };

        let arrived = NaiveDate::from_ymd_opt(2026, 10, 14)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        // 4 minutes later
        let started = NaiveDate::from_ymd_opt(2026, 10, 14)
            .unwrap()
            .and_hms_opt(12, 4, 30)
            .unwrap();

        let (waited, billable_mins, charge) = calculate_waiting_charge(&opt, arrived, started);
        assert_eq!(waited, 270);
        assert_eq!(billable_mins, 0);
        assert_eq!(charge, 0);

        // 8 minutes later (3 additional minutes)
        let started_late = NaiveDate::from_ymd_opt(2026, 10, 14)
            .unwrap()
            .and_hms_opt(12, 8, 0)
            .unwrap();
        let (waited_late, billable_late, charge_late) =
            calculate_waiting_charge(&opt, arrived, started_late);
        assert_eq!(waited_late, 480);
        assert_eq!(billable_late, 3);
        assert_eq!(charge_late, 3 * 3000);
    }
}
