/// Inserts a value into the specified database table using Diesel.
///
/// # Arguments
/// * `$table` - The path to the Diesel table into which the value will be inserted.
/// * `$value` - The value to insert. Should implement the appropriate Diesel traits for insertion.
/// * `$conn` - The database connection to use for the operation.
///
/// # Errors
/// Returns a `ModuleError::InternalError` if the insertion fails, including the error message and table name.
///
/// # Example
/// ```rust
/// let mut conn = pool.get().await.map_err(|e| ModuleError::InternalError(e.to_string()))?;
/// let user_task = UserTasks { user_id: payload.user_id,task_id: payload.subject};
/// insert!(user_tasks::table, user_task, conn);
/// ```

/// Inserts a value into a table, with optional ON CONFLICT handling.
///
/// Variants:
/// - insert!(table, value, conn): Insert with do-nothing on any conflict (immutable conn).
/// - insert!(table, value, conn, on_conflict: (col1, col2, ...)): Do-nothing on conflict on multiple columns.
/// - insert!(table, value, conn, on_conflict: col): Do-nothing on conflict on a single column/expression.
/// - insert!(table, value, &mut conn): Same as first variant but accepts a mutable connection reference.
/// - insert!(table, value, &mut conn, on_conflict: (..)) and insert!(table, value, &mut conn, on_conflict: col): Mutable conn variants.
///
/// Errors:
/// - Propagates ModuleError::InternalError on insertion failure.
#[macro_export]
macro_rules! insert {
    ($table:path, $value:ident, $conn:ident) => {
        diesel::insert_into($table)
            .values(&$value)
            .on_conflict_do_nothing()
            .execute($conn)
            .await
            .map_err(|e| {
                ModuleError::InternalError(format!(
                    "Error Inserting into {}",
                    e
                ))
            })?;
    };

    // Multiple columns conflict
    ($table:path, $value:ident, $conn:ident, on_conflict: ($($col:expr),+ $(,)?)) => {
        diesel::insert_into($table)
            .values(&$value)
            .on_conflict(($($col,)+))
            .do_nothing()
            .execute($conn)
            .await
            .map_err(|e| {
                ModuleError::InternalError(format!(
                    "Error Inserting into {}",
                    e
                ).into())
            })?;
    };

    // Single column with explicit conflict handling
    ($table:path, $value:ident, $conn:ident, on_conflict: $col:expr) => {
        diesel::insert_into($table)
            .values(&$value)
            .on_conflict($col)
            .do_nothing()
            .execute($conn)
            .await
            .map_err(|e| {
                ModuleError::InternalError(format!(
                    "Error Inserting into {}",
                    e
                ).into())
            })?;
    };

    // With mutable reference variants
    ($table:path, $value:ident, &mut $conn:ident) => {
        diesel::insert_into($table)
            .values(&$value)
            .on_conflict_do_nothing()
            .execute(&mut *$conn)
            .await
            .map_err(|e| {
                ModuleError::InternalError(format!(
                    "Error Inserting into {}",
                    e
                ).into())
            })?;
    };

    ($table:path, $value:ident, &mut $conn:ident, on_conflict: ($($col:expr),+ $(,)?)) => {
        diesel::insert_into($table)
            .values(&$value)
            .on_conflict(($($col,)+))
            .do_nothing()
            .execute(&mut *$conn)
            .await
            .map_err(|e| {
                ModuleError::InternalError(format!(
                    "Error Inserting into {}",
                    e
                ).into())
            })?;
    };

    ($table:path, $value:ident, &mut $conn:ident, on_conflict: $col:expr) => {
        diesel::insert_into($table)
            .values(&$value)
            .on_conflict($col)
            .do_nothing()
            .execute(&mut *$conn)
            .await
            .map_err(|e| {
                ModuleError::InternalError(format!(
                    "Error Inserting into {}",
                    e
                ).into())
            })?;
    };
}

/// Updates a record in the specified database table using Diesel ORM.
///
/// # Arguments
/// * `$table` - The path to the database table to update.
/// * `$value` - The struct containing updated values to set.
/// * `$conn` - The database connection to use for the update.
///
/// # Errors
/// Returns a `ModuleError::InternalError` if the update operation fails, including the error message and table name.
///
/// # Example
/// ```rust
/// update!(users::table, updated_user, conn);
/// ```
#[macro_export]
macro_rules! update {
    ($table:path, $value:ident, $conn:ident) => {
        diesel::update($table)
            .set(&$value)
            .execute($conn)
            .await
            .map_err(|e| ModuleError::InternalError(format!("Error Updating {}", e).into()))?;
    };
}

/// Macro to simplify fetching data from a database table using Diesel ORM.
///
/// # Variants
///
/// ## 1. Fetch all rows from a table
/// ```rust
/// fetch!(table_path, ReturnType, conn)
/// ```
/// - `table_path`: Path to the Diesel table.
/// - `ReturnType`: Type to map the result to (must implement `Selectable`).
/// - `conn`: Database connection (must be mutable).
///
/// Returns a `Vec<ReturnType>` or propagates a `ModuleError::InternalError` on failure.
///
/// ## 2. Fetch a single row by filter
/// ```rust
/// fetch!(table_path, filter_path, value, ReturnType, conn)
/// ```
/// - `table_path`: Path to the Diesel table.
/// - `filter_path`: Path to the column to filter by.
/// - `value`: Value to match for the filter.
/// - `ReturnType`: Type to map the result to (must implement `Selectable`).
/// - `conn`: Database connection (must be mutable).
///
/// Returns a single `ReturnType` if found, or propagates a `ModuleError::ItemNotFound` if not found,
/// or a `ModuleError::InternalError` on query failure.
///
/// # Errors
/// - Returns `ModuleError::InternalError` if the database query fails.
/// - Returns `ModuleError::ItemNotFound` if no matching item is found (for the filtered variant).
#[macro_export]
macro_rules! fetch {
    // used in db transactions
    ($table:path, $return_type:ty, $conn:ident) => {{
        use diesel::{QueryDsl, SelectableHelper};
        use diesel_async::RunQueryDsl;
        $table
            .select(<$return_type>::as_select())
            .load($conn)
            .await
            .map_err(|e| ModuleError::InternalError(format!("Error fetching data: {}", e)))?
    }};
    ($table:path, $return_type:ty, &mut $conn:ident) => {{
        use diesel::{QueryDsl, SelectableHelper};
        use diesel_async::RunQueryDsl;
        $table
            .select(<$return_type>::as_select())
            .load(&mut *$conn)
            .await
            .map_err(|e| {
                ModuleError::InternalError(format!("Error fetching item:  {}", e).into())
            })?
    }};
    // used in db transactions
    ($table:path, $filter:path, $value:expr, $return_type:ty, $conn:ident) => {{
        use diesel::OptionalExtension;
        $table
            .filter($filter.eq($value))
            .select(<$return_type>::as_select())
            .first($conn)
            .await
            .optional()
            .map_err(|e| ModuleError::InternalError(format!("Error fetching item: {}", e).into()))?
    }};

    ($table:path, $filter:path, $value:expr, $return_type:ty, &mut $conn:ident) => {{
        use diesel::OptionalExtension;
        $table
            .filter($filter.eq($value))
            .select(<$return_type>::as_select())
            .first(&mut *$conn)
            .await
            .optional()
            .map_err(|e| ModuleError::InternalError(format!("Error fetching item: {}", e).into()))?
    }};
}
