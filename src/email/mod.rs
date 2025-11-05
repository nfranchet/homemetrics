pub mod common;
pub mod processor_base;

// Re-export commonly used items
pub use common::{EmailInfo, ProcessingResult};
pub use processor_base::{EmailProcessingStrategy, BaseEmailProcessor};
