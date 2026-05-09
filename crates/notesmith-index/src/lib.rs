//! notesmith-index: SQLite cache builder and Tantivy full-text search indexing

pub mod cache;
pub mod indexer;
pub mod schema;

pub use cache::VaultCache;
pub use indexer::CacheIndexer;
