//! notesmith-index: SQLite cache builder and Tantivy full-text search indexing

pub mod cache;
pub mod field_registry;
pub mod indexer;
pub mod schema;
pub mod search;
pub mod user_views;

pub use cache::VaultCache;
pub use field_registry::{FieldDefinition, FieldRegistry, FieldType};
pub use indexer::CacheIndexer;
pub use search::{SearchIndex, SearchResult};
