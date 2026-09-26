// @generated automatically by Diesel CLI.

diesel::table! {
    users (id) {
        id -> Uuid,
        first_name -> Text,
        last_name -> Text,
        email -> Text,
        username -> Text,
        phone -> Text,
        address -> Text,
        city -> Text,
        country -> Text,
        identity_type -> Nullable<Text>,
        identity_number -> Nullable<Text>,
        password_hash -> Text,
        is_identity_verified -> Bool,
        role -> Text,
        is_active -> Bool,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        last_seen -> Timestamp,
    }
}

diesel::table! {
    buses (id) {
        id -> Uuid,
        license_plate -> Text,
        capacity -> Integer,
        current_passenger_count -> Integer,
        current_trip_id -> Nullable<Uuid>,
        state -> Text,
        status -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    points (id) {
        id -> Uuid,
        name -> Text,
        geo_location -> Jsonb,
        landmark -> Text,
        description -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    routes (id) {
        id -> Uuid,
        name -> Text,
        description -> Text,
        start_point -> Uuid,
        end_point -> Uuid,
        fare -> Float,
        distance -> Float,
        stops -> Array<Uuid>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    activity_logs (id) {
        id -> Uuid,
        user_id -> Uuid,
        activity_type -> Text,
        target_id -> Nullable<Uuid>,
        target_type -> Nullable<Text>,
        details -> Jsonb,
        ip_address -> Nullable<Text>,
        user_agent -> Nullable<Text>,
        created_at -> Timestamp,
    }
}

diesel::table! {
    bookings (id) {
        id -> Uuid,
        booking_ref -> Text,
        user_id -> Uuid,
        bus_id -> Uuid,
        route_id -> Uuid,
        seat_number -> Integer,
        status -> Text,
        payment_status -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        qr_code -> Nullable<Text>,
        trip_id -> Nullable<Uuid>,
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
        ride_type -> Text,
        status -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    ride_requests (id) {
        id -> Uuid,
        route_id -> Uuid,
        count -> Integer,
        departure_date -> Date,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    users,
    buses,
    points,
    routes,
    activity_logs,
    bookings,
    trips,
    ride_requests
);

