//! OpenAPI endpoint registration and authentication schemes for the admin API.
#[derive(utoipa::OpenApi)]
#[openapi(paths(super::handlers::safety::log_call,super::handlers::rides::rides,super::handlers::rides::ride,super::handlers::safety::sos,super::handlers::safety::acknowledge,super::handlers::safety::resolve,super::handlers::safety::incidents,super::handlers::safety::incident,super::handlers::safety::create_incident,super::handlers::safety::update_incident,super::handlers::safety::add_note,super::handlers::people::drivers,super::handlers::people::live_drivers,super::handlers::people::driver,super::handlers::people::driver_rides,super::handlers::people::flag,super::handlers::people::unflag,super::handlers::people::deactivate,super::handlers::people::passengers,super::handlers::people::passenger,super::handlers::people::passenger_rides,super::handlers::people::restrict,super::handlers::people::unrestrict,super::handlers::people::membership_check,super::handlers::quality::quality,super::handlers::quality::clear_flag,super::handlers::account::notifications,super::handlers::account::contacts,super::handlers::account::add_contact,super::handlers::account::remove_contact,super::handlers::applications::list,super::handlers::applications::detail,super::handlers::applications::document,super::handlers::applications::comment,super::handlers::applications::inspection,super::handlers::applications::driver_inspection,super::handlers::applications::decision,super::handlers::applications::own_review,super::handlers::reports::overview,super::handlers::reports::reports,super::handlers::reports::export),modifiers(&AdminSecurity))]
pub struct AdminApiDoc;
struct AdminSecurity;
impl utoipa::Modify for AdminSecurity {
    /// Register Bearer-token and access-token-cookie authentication in Swagger.
    fn modify(&self, doc: &mut utoipa::openapi::OpenApi) {
        use utoipa::openapi::security::*;
        let components = doc.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearerAuth",
            SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
        );
        components.add_security_scheme(
            "cookieAuth",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new("access_token"))),
        );
    }
}
