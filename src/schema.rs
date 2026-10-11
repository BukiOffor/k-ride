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
    }
}

diesel::joinable!(activity_logs -> users (user_id));
diesel::joinable!(bookings -> buses (bus_id));
diesel::joinable!(bookings -> routes (route_id));
diesel::joinable!(bookings -> trips (trip_id));
diesel::joinable!(bookings -> users (user_id));
diesel::joinable!(ride_requests -> routes (route_id));
diesel::joinable!(trips -> routes (route_id));
diesel::joinable!(trips -> users (driver_id));

diesel::allow_tables_to_appear_in_same_query!(
    activity_logs,
    bookings,
    buses,
    points,
    ride_requests,
    routes,
    trips,
    users,
);
