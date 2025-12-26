mod extension_pg;

use anyhow::Error;
pub use extension_pg::PostgresExtension;

pub trait DatabaseExtension {
    async fn fetch(&self, doc_id: &str) -> Result<Vec<u8>, Error>;

    async fn store(&self, doc_id: &str, state: Vec<u8>) -> Result<(), Error>;

}
