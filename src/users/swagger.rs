use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        super::handlers::create_user,
        super::handlers::assign_role_to_user,
        super::handlers::register_user,
        super::handlers::get_users,
        super::handlers::get_user_by_id,
        super::handlers::update_user,
        super::handlers::activate_user,
        super::handlers::deactivate_user,
        super::handlers::delete_user,
    ),
    components(schemas(
        super::dto::CreateUserDto,
        super::dto::AdminCreateUserDto,
        super::dto::AssignRoleDto,
        super::dto::GetUsersQuery,
        super::dto::UpdateUserDto,
        super::model::User,
        super::UserDto,
    ))
)]
pub struct UsersApiDoc;
