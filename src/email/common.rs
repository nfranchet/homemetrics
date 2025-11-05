/// Common structures and utilities for email processing
use chrono::{DateTime, Utc};

/// Email information retrieved from Gmail
#[derive(Debug, Clone)]
pub struct EmailInfo {
    pub subject: String,
    pub content: Vec<u8>,
    pub date: DateTime<Utc>,
    pub headers: String,
    pub id: String,
}

/// Result of email processing
#[derive(Debug)]
pub struct ProcessingResult {
    pub emails_processed: usize,
    pub emails_failed: usize,
}

impl ProcessingResult {
    pub fn new() -> Self {
        Self {
            emails_processed: 0,
            emails_failed: 0,
        }
    }
    
    pub fn success(&mut self) {
        self.emails_processed += 1;
    }
    
    pub fn failure(&mut self) {
        self.emails_failed += 1;
    }
}

impl Default for ProcessingResult {
    fn default() -> Self {
        Self::new()
    }
}
