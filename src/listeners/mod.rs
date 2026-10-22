pub mod pumpportal;

use async_trait::async_trait;
use anyhow::Result;

#[async_trait]
pub trait LaunchpadListener: Send + Sync {
    /// Start listening to launches and trades in an async background loop
    async fn run(&self) -> Result<()>;
}
