/// Blue Riot pool monitoring email processing module
pub mod extractor;
pub mod processor;

pub use extractor::PoolReading;
pub use processor::BlueRiotEmailProcessor;
