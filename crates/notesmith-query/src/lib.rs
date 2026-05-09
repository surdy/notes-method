//! notesmith-query: Stable SQL views, query execution, and dashboard helpers

pub mod executor;

pub use executor::{QueryError, QueryResult, execute_sql};
