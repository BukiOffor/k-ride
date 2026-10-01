use super::super::models::bookings::{BookingStatus, PaymentStatus};
use super::*;
use crate::types::Pagination;

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateBookingDto {
    pub bus_id: Uuid,
    pub route_id: Uuid,
    pub trip_id: Option<Uuid>,
    pub seat_number: Option<i32>,
    pub user_id: Option<Uuid>,
    pub pickup_point: Option<Uuid>,
    pub dropoff_point: Option<Uuid>,
    pub num_of_kids: i32, 
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

/// Response containing a base64-encoded QR code PNG image for a booking pass.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BookingQrCodeResponse {
    /// The booking ID encoded in the QR code
    pub booking_id: Uuid,
    /// The human-readable booking reference
    pub booking_ref: String,
    /// Base64-encoded PNG image of the QR code
    pub qr_code_base64: String,
}

/// Payload sent by the admin/conductor after scanning a QR code.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct VerifyBookingDto {
    /// The raw string content extracted from the scanned QR code
    pub qr_payload: String,
}

/// Result returned after verifying a scanned QR code booking pass.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BookingVerificationResult {
    pub valid: bool,
    pub booking_id: Option<Uuid>,
    pub booking_ref: Option<String>,
    pub status: Option<BookingStatus>,
    pub payment_status: Option<PaymentStatus>,
    pub seat_number: Option<i32>,
    pub message: String,
}
