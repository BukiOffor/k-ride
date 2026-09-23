use super::super::models::bookings::{BookingStatus, PaymentStatus};
use super::*;
use crate::types::Pagination;

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateBookingDto {
    pub bus_id: Uuid,
    pub route_id: Uuid,
    pub seat_number: Option<i32>,
    pub user_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateBookingStatusDto {
    pub status: BookingStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdatePaymentStatusDto {
    pub payment_status: PaymentStatus,
}

#[derive(Debug, Clone, Default, Deserialize, utoipa::IntoParams, utoipa::ToSchema)]
pub struct GetBookingsQuery {
    pub bus_id: Option<Uuid>,
    pub route_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub status: Option<BookingStatus>,
    pub payment_status: Option<PaymentStatus>,
    pub booking_ref: Option<String>,
    pub pagination: Option<Pagination>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BusSeatsResponseDto {
    pub bus_id: Uuid,
    pub capacity: i32,
    pub current_passenger_count: i32,
    pub booked_seats: Vec<i32>,
    pub available_seats: Vec<i32>,
}
