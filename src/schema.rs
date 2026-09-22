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
        current_route_id -> Nullable<Uuid>,
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
        departure_time -> Time,
        departure_date -> Date,
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

diesel::allow_tables_to_appear_in_same_query!(
    users,
    buses,
    points,
    routes,
    activity_logs
);
