use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, utoipa::ToSchema, utoipa::IntoParams)]
pub struct Pagination {
    #[serde(default = "default_page")]
    pub page: i32,
    #[serde(default = "default_page_size")]
    pub size: i32,
}

fn default_page() -> i32 {
    1
}
fn default_page_size() -> i32 {
    10
}

impl Pagination {
    pub fn offset(&self) -> i32 {
        (self.page - 1) * self.size
    }
}

#[derive(Serialize, Deserialize, utoipa::ToSchema)]
pub struct PaginatedResult<T> {
    pub items: Vec<T>,
    pub metadata: Metadata,
}

#[derive(Serialize, Deserialize, utoipa::ToSchema)]
pub struct Metadata {
    pub page: i32,
    pub size: i32,
    pub total_items: i32,
    pub num_pages: i32,
}

impl<T> PaginatedResult<T> {
    pub fn new(items: Vec<T>, total_items: i32, pagination: Pagination) -> Self {
        let metadata = Metadata {
            page: pagination.page,
            size: pagination.size,
            total_items,
            num_pages: (total_items as f64 / pagination.size as f64).ceil() as i32,
        };
        Self { items, metadata }
    }
}

#[derive(Serialize, Deserialize, utoipa::ToSchema)]
pub struct PaginatedResultWithContext<T, R: Serialize> {
    pub items: Vec<T>,
    pub metadata: MetadataWithContext<R>,
}

#[derive(Serialize, Deserialize, utoipa::ToSchema)]
pub struct MetadataWithContext<R: Serialize + Sized> {
    pub page: i32,
    pub size: i32,
    pub total_items: i32,
    pub num_pages: i32,
    pub context: Option<R>,
}

impl<T, R: Serialize + Sized> PaginatedResultWithContext<T, R> {
    pub fn new(items: Vec<T>, total_items: i32, pagination: Pagination) -> Self {
        let metadata = MetadataWithContext {
            page: pagination.page,
            size: pagination.size,
            total_items,
            num_pages: (total_items as f64 / pagination.size as f64).ceil() as i32,
            context: None,
        };
        Self { items, metadata }
    }

    pub fn set_context(&mut self, context: R) -> &mut Self {
        self.metadata.context = Some(context);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, utoipa::ToSchema, utoipa::IntoParams)]
pub struct TaskFilterQuery {
    #[serde(default = "default_page")]
    pub page: i32,
    #[serde(default = "default_page_size")]
    pub size: i32,
    pub search: Option<String>,
    pub todo: Option<bool>,
    pub done: Option<bool>,
    pub doing: Option<bool>,
    pub assigned: Option<bool>,
    pub unassigned: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, utoipa::ToSchema)]
pub struct PaginationWithContext<T> {
    #[serde(default = "default_page")]
    pub page: i32,
    #[serde(default = "default_page_size")]
    pub size: i32,
    pub search: Option<String>,
    pub filter: Option<String>,
    pub context: T,
}

impl<T> From<PaginationWithContext<T>> for Pagination {
    fn from(value: PaginationWithContext<T>) -> Self {
        Self {
            page: value.page,
            size: value.size,
        }
    }
}
