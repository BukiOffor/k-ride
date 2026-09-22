use super::*;


#[derive(Debug, Clone, Queryable, Identifiable, Serialize, Deserialize, Insertable, Selectable)]
#[diesel(primary_key(id))]
#[diesel(table_name = crate::schema::activity_logs)]
pub struct ActivityLog {
    pub id: Uuid,
    pub user_id: Uuid,
    pub activity_type: ActivityType,
    pub target_id: Option<Uuid>, // receiptent of the operation <topic id or subject id or user id>
    pub target_type: Option<String>, // change to enum ??
    pub details: Value,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: NaiveDateTime,
}


#[derive(Debug, Clone, Serialize, Deserialize, AsExpression, FromSqlRow, PartialEq, Eq)]
#[diesel(sql_type = diesel::sql_types::Text)]
pub enum ActivityType {
    AccountCreated,
    UserLogin,
    UserLogout,
    UserCreated,
    UserUpdated,
    UserActivation,
    UserDeactivation,
    UserDeleted,
}

impl FromSql<Text, diesel::pg::Pg> for ActivityType {
    fn from_sql(bytes: diesel::pg::PgValue<'_>) -> deserialize::Result<Self> {
        let s = std::str::from_utf8(bytes.as_bytes())?;
        serde_json::from_str(s).map_err(Into::into)
    }
}

impl ToSql<Text, diesel::pg::Pg> for ActivityType {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        let s = serde_json::to_string(self)?;
        out.write(s.as_bytes())?;
        Ok(serialize::IsNull::No)
    }
}

impl ActivityLog {
    pub fn new(activity_type: ActivityType, user_id: Uuid) -> Self {
        ActivityLog {
            id: Uuid::now_v7(),
            user_id,
            activity_type,
            target_id: None,
            target_type: None,
            details: Value::Null,
            ip_address: None,
            user_agent: None,
            created_at: chrono::Local::now().naive_local(),
        }
    }
    pub fn set_target_id(&mut self, target_id: Uuid) -> &mut Self {
        self.target_id = Some(target_id);
        self
    }
    pub fn set_target_type(&mut self, target_type: String) -> &mut Self {
        self.target_type = Some(target_type);
        self
    }

    pub fn set_ip_address(&mut self, ip_address: String) -> &mut Self {
        self.ip_address = Some(ip_address);
        self
    }

    pub fn set_user_agent(&mut self, user_agent: String) -> &mut Self {
        self.user_agent = Some(user_agent);
        self
    }

    pub fn set_details(&mut self, details: Value) -> &mut Self {
        self.details = details;
        self
    }
    /// resets the ID to a new UUID
    /// this is useful when you want to reuse the same log structure for a new log entry
    pub fn reset_id(&mut self) -> &mut Self {
        self.id = Uuid::now_v7();
        self
    }
    pub fn finish(&self) -> Self {
        self.clone()
    }
}

impl ActivityType {
    pub fn message(&self) -> String {
        match self {
            ActivityType::UserLogin => "Logged into the system.".into(),
            ActivityType::UserLogout => "Logged out of the system.".into(),
            Self::AccountCreated => "Account created ".into(),
            ActivityType::UserCreated => "Created a new user.".into(),
            ActivityType::UserUpdated => "Updated a user's information.".into(),
            ActivityType::UserActivation => "Activated a user account.".into(),
            ActivityType::UserDeactivation => "Deactivated a user account.".into(),
            ActivityType::UserDeleted => "Deleted a user.".into(),          
        }
    }
}