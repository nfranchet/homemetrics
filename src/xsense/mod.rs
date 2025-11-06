/// X-Sense temperature sensor email processing module
pub mod extractor;
pub mod processor;

pub use extractor::{TemperatureReading, TemperatureExtractor};
pub use processor::XSenseEmailProcessor;
