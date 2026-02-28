pub mod memory;
pub mod sqlite;

pub use memory::MemoryStorage;
pub use sqlite::SqliteStorage;

use pacinet_core::Storage;
use std::sync::Arc;
use tonic::Status;

/// Wrap a synchronous Storage call in `spawn_blocking` for async contexts.
pub async fn blocking<F, T>(storage: &Arc<dyn Storage>, f: F) -> Result<T, Status>
where
    F: FnOnce(&dyn Storage) -> Result<T, pacinet_core::PaciNetError> + Send + 'static,
    T: Send + 'static,
{
    let storage = storage.clone();
    tokio::task::spawn_blocking(move || f(storage.as_ref()))
        .await
        .map_err(|e| Status::internal(format!("spawn_blocking failed: {}", e)))?
        .map_err(Status::from)
}
