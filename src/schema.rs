// @generated automatically by Diesel CLI.

diesel::table! {
    activity_logs (id) {
        id -> Uuid,
        user_id -> Uuid,
        #[max_length = 100]
        activity_type -> Varchar,
        target_id -> Nullable<Uuid>,
        #[max_length = 100]
        target_type -> Nullable<Varchar>,
        details -> Jsonb,
        #[max_length = 100]
        ip_address -> Nullable<Varchar>,
        user_agent -> Nullable<Text>,
        created_at -> Timestamp,
    }
}

diesel::table! {
    bookings (id) {
        id -> Uuid,
        #[max_length = 100]
        booking_ref -> Varchar,
        user_id -> Uuid,
        bus_id -> Uuid,
        route_id -> Uuid,
        seat_number -> Int4,
        #[max_length = 50]
        status -> Varchar,
        #[max_length = 50]
        payment_status -> Varchar,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        qr_code -> Nullable<Text>,
        trip_id -> Nullable<Uuid>,
        pickup_point -> Nullable<Uuid>,
        dropoff_point -> Nullable<Uuid>,
        num_of_kids -> Int4,
    }
}

diesel::table! {
    buses (id) {
        id -> Uuid,
        #[max_length = 50]
        license_plate -> Varchar,
        capacity -> Int4,
        current_passenger_count -> Int4,
        current_trip_id -> Nullable<Uuid>,
        #[max_length = 50]
        state -> Varchar,
        #[max_length = 50]
        status -> Varchar,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    points (id) {
        id -> Uuid,
        #[max_length = 255]
        name -> Varchar,
        geo_location -> Jsonb,
        landmark -> Text,
        description -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    ride_requests (id) {
        id -> Uuid,
        route_id -> Uuid,
        count -> Int4,
        departure_date -> Date,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    routes (id) {
        id -> Uuid,
        #[max_length = 255]
        name -> Varchar,
        description -> Text,
        start_point -> Uuid,
        end_point -> Uuid,
        fare -> Float4,
        distance -> Float4,
        stops -> Array<Nullable<Uuid>>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    trips (id) {
        id -> Uuid,
        route_id -> Uuid,
        bus_id -> Nullable<Uuid>,
        driver_id -> Nullable<Uuid>,
        departure_time -> Timestamp,
        arrival_time -> Timestamp,
        actual_departure_time -> Nullable<Timestamp>,
        actual_arrival_time -> Nullable<Timestamp>,
        #[max_length = 50]
        ride_type -> Varchar,
        #[max_length = 50]
        status -> Varchar,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        return_time -> Nullable<Timestamp>,
    }
}

diesel::table! {
    users (id) {
        id -> Uuid,
        #[max_length = 100]
        first_name -> Varchar,
        #[max_length = 100]
        last_name -> Varchar,
        #[max_length = 255]
        email -> Varchar,
        #[max_length = 100]
        username -> Varchar,
        #[max_length = 50]
        phone -> Varchar,
        address -> Text,
        #[max_length = 100]
        country -> Varchar,
        #[max_length = 100]
        identity_type -> Nullable<Varchar>,
        #[max_length = 100]
        identity_number -> Nullable<Varchar>,
        password_hash -> Text,
        is_identity_verified -> Bool,
        #[max_length = 50]
        role -> Varchar,
        is_active -> Bool,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        last_seen -> Timestamp,
        #[max_length = 100]
        city -> Varchar,
        car -> Nullable<Uuid>,
        #[max_length = 10]
        account_number -> Nullable<Varchar>,
        bank_name -> Nullable<Varchar>,
    }
}

diesel::table! {
    cars(id) {
        id -> Uuid,
        #[max_length = 50]
        license_plate -> Varchar,
        capacity -> Int4,
        car_type -> Varchar,
        car_tier -> Varchar,
        car_size -> Varchar,
        #[max_length = 50]
        color -> Varchar,
        #[max_length = 20]
        color_hex -> Nullable<Varchar>,
        #[max_length = 100]
        brand -> Varchar,
        #[max_length = 100]
        model -> Varchar,
        year -> Int4,
        vehicle_photo -> Nullable<Text>,
        vehicle_registration -> Nullable<Text>,
        insurance_certificate -> Nullable<Text>,
        roadworthiness_certificate -> Nullable<Text>,
        is_verified -> Bool,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    driver_profiles (id) {
        id -> Uuid,
        user_id -> Uuid,
        car_id -> Uuid,
        nin_slip -> Text,
        profile_photo -> Text,
        is_koinonia_worker -> Bool,
        #[max_length = 255]
        koinonia_department -> Nullable<Varchar>,
        #[max_length = 255]
        guarantor_name -> Nullable<Varchar>,
        #[max_length = 50]
        guarantor_phone -> Nullable<Varchar>,
        #[max_length = 100]
        guarantor_relationship -> Nullable<Varchar>,
        guarantor_photo -> Nullable<Text>,
        #[max_length = 100]
        licence_number -> Varchar,
        #[max_length = 10]
        licence_class -> Varchar,
        licence_expiry_date -> Date,
        licence_photo -> Text,
        #[max_length = 50]
        track -> Varchar,
        #[max_length = 50]
        status -> Varchar,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    ride_options (id) {
        #[max_length = 50]
        id -> Varchar,
        #[max_length = 100]
        name -> Varchar,
        description -> Text,
        #[max_length = 20]
        vehicle_class -> Varchar,
        eligible_vehicle_classes -> Array<Text>,
        capacity -> Int4,
        shared -> Bool,
        #[max_length = 3]
        currency -> Varchar,
        base_fare_minor -> Int8,
        per_km_minor -> Int8,
        per_minute_minor -> Int8,
        minimum_fare_minor -> Int8,
        booking_fee_minor -> Int8,
        free_waiting_minutes -> Int4,
        waiting_per_minute_minor -> Int8,
        rounding_minor -> Int8,
        is_active -> Bool,
        sort_order -> Int4,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    fare_estimates (id) {
        id -> Uuid,
        user_id -> Uuid,
        pickup -> Jsonb,
        destination -> Jsonb,
        route -> Jsonb,
        options -> Jsonb,
        expires_at -> Timestamp,
        created_at -> Timestamp,
    }
}

diesel::table! {
    rides (id) {
        id -> Uuid,
        passenger_id -> Uuid,
        driver_id -> Nullable<Uuid>,
        #[max_length = 30]
        status -> Varchar,
        version -> Int4,
        #[max_length = 50]
        option_id -> Varchar,
        option_snapshot -> Jsonb,
        estimate_id -> Uuid,
        pickup -> Jsonb,
        destination -> Jsonb,
        route -> Jsonb,
        fare -> Jsonb,
        #[max_length = 3]
        currency -> Varchar,
        fare_total_minor -> Int8,
        waiting_charge_minor -> Int8,
        final_fare_minor -> Nullable<Int8>,
        #[max_length = 30]
        payment_method -> Varchar,
        #[max_length = 30]
        payment_status -> Varchar,
        #[max_length = 8]
        trip_pin -> Varchar,
        dispatch_round -> Int4,
        cancellation -> Nullable<Jsonb>,
        tracking -> Nullable<Jsonb>,
        rating -> Nullable<Jsonb>,
        #[max_length = 20]
        sos_status -> Varchar,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        assigned_at -> Nullable<Timestamp>,
        arrived_at -> Nullable<Timestamp>,
        started_at -> Nullable<Timestamp>,
        completed_at -> Nullable<Timestamp>,
        cancelled_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    ride_offers (id) {
        id -> Uuid,
        ride_id -> Uuid,
        driver_id -> Uuid,
        dispatch_round -> Int4,
        #[max_length = 20]
        status -> Varchar,
        pickup_distance_m -> Nullable<Int4>,
        pickup_eta_seconds -> Nullable<Int4>,
        expires_at -> Timestamp,
        responded_at -> Nullable<Timestamp>,
        created_at -> Timestamp,
    }
}

diesel::table! {
    driver_availability (driver_id) {
        driver_id -> Uuid,
        #[max_length = 20]
        status -> Varchar,
        lat -> Nullable<Float8>,
        lng -> Nullable<Float8>,
        heading -> Nullable<Float8>,
        speed_mps -> Nullable<Float8>,
        accuracy_m -> Nullable<Float8>,
        recorded_at -> Nullable<Timestamp>,
        received_at -> Nullable<Timestamp>,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    ride_ratings (id) {
        id -> Uuid,
        ride_id -> Uuid,
        rater_id -> Uuid,
        ratee_id -> Uuid,
        #[max_length = 20]
        rater_role -> Varchar,
        stars -> Int2,
        tags -> Array<Text>,
        comment -> Nullable<Text>,
        created_at -> Timestamp,
    }
}

diesel::table! {
    safety_events (id) {
        id -> Uuid,
        ride_id -> Uuid,
        reporter_id -> Uuid,
        #[max_length = 20]
        reporter_role -> Varchar,
        #[max_length = 20]
        kind -> Varchar,
        #[max_length = 50]
        event_type -> Varchar,
        details -> Nullable<Text>,
        location -> Nullable<Jsonb>,
        driver_location -> Nullable<Jsonb>,
        #[max_length = 20]
        status -> Varchar,
        occurred_at -> Timestamp,
        created_at -> Timestamp,
    }
}

diesel::table! {
    idempotency_keys (user_id, endpoint, idempotency_key) {
        user_id -> Uuid,
        #[max_length = 100]
        endpoint -> Varchar,
        #[max_length = 200]
        idempotency_key -> Varchar,
        status_code -> Int4,
        response -> Jsonb,
        created_at -> Timestamp,
    }
}

diesel::joinable!(activity_logs -> users (user_id));
diesel::joinable!(bookings -> buses (bus_id));
diesel::joinable!(bookings -> routes (route_id));
diesel::joinable!(bookings -> trips (trip_id));
diesel::joinable!(bookings -> users (user_id));
diesel::joinable!(driver_profiles -> cars (car_id));
diesel::joinable!(driver_profiles -> users (user_id));
diesel::joinable!(ride_requests -> routes (route_id));
diesel::joinable!(trips -> routes (route_id));
diesel::joinable!(trips -> users (driver_id));
diesel::joinable!(users -> cars (car));
diesel::joinable!(rides -> ride_options (option_id));
diesel::joinable!(ride_offers -> rides (ride_id));
diesel::joinable!(ride_ratings -> rides (ride_id));
diesel::joinable!(safety_events -> rides (ride_id));

diesel::allow_tables_to_appear_in_same_query!(
    activity_logs,
    bookings,
    buses,
    points,
    ride_requests,
    routes,
    trips,
    users,
    cars,
    driver_profiles,
    ride_options,
    fare_estimates,
    rides,
    ride_offers,
    driver_availability,
    ride_ratings,
    safety_events,
    idempotency_keys,
    scheduled_rides,
    email_outbox,
    scheduled_events,
);

// Scheduled bookings and durable delivery queues (2026-10-22 migration).
diesel::table! {
    scheduled_rides (id) {
        id -> Uuid,
        passenger_id -> Uuid,
        driver_id -> Nullable<Uuid>,
        estimate_id -> Uuid,
        option_id -> Text,
        option_snapshot -> Jsonb,
        pickup -> Jsonb,
        destination -> Jsonb,
        route -> Jsonb,
        fare -> Jsonb,
        currency -> Text,
        fare_total_minor -> Int8,
        scheduled_at -> Timestamp,
        timezone -> Text,
        duration_seconds -> Int4,
        buffer_minutes -> Int4,
        status -> Text,
        ride_id -> Nullable<Uuid>,
        version -> Int4,
        cancellation_reason -> Nullable<Text>,
        next_broadcast_at -> Nullable<Timestamp>,
        reminder_sent -> Bool,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}
diesel::table! {
    email_outbox (id) {
        sequence_id -> Int8,
        id -> Uuid,
        aggregate_id -> Uuid,
        event_key -> Text,
        recipient_id -> Uuid,
        recipient -> Text,
        subject -> Text,
        body -> Text,
        transport -> Text,
        attempts -> Int4,
        next_attempt_at -> Timestamp,
        lease_token -> Nullable<Uuid>,
        lease_until -> Nullable<Timestamp>,
        sent_at -> Nullable<Timestamp>,
        dead_at -> Nullable<Timestamp>,
        provider_message_id -> Nullable<Text>,
        last_error -> Nullable<Text>,
        created_at -> Timestamp,
    }
}
diesel::table! {
    scheduled_events (id) {
        sequence_id -> Int8,
        id -> Uuid,
        scheduled_ride_id -> Uuid,
        kind -> Text,
        payload -> Jsonb,
        broadcast -> Bool,
        driver_ids -> Array<Uuid>,
        created_at -> Timestamp,
        delivered_at -> Nullable<Timestamp>,
        lease_token -> Nullable<Uuid>,
        lease_until -> Nullable<Timestamp>,
    }
}
