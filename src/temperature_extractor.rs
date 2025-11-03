use anyhow::{Result, Context};
use chrono::{DateTime, Utc, NaiveDateTime};
use csv::ReaderBuilder;
use log::{info, debug, warn};
use regex::Regex;
use serde::{Deserialize, Serialize};


use crate::attachment_parser::Attachment;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemperatureReading {
    pub sensor_id: String,
    pub timestamp: DateTime<Utc>,
    pub temperature: f64,
    pub humidity: Option<f64>,
    pub location: Option<String>,
}

pub struct TemperatureExtractor;

impl TemperatureExtractor {
    pub fn extract_from_attachment(attachment: &Attachment) -> Result<Vec<TemperatureReading>> {
        info!("Extracting temperature data from: {}", attachment.filename);
        
        // Extract sensor name from filename
        let sensor_name = Self::extract_sensor_name(&attachment.filename)?;
        debug!("Extracted sensor name: {}", sensor_name);
        
        match attachment.filename.to_lowercase() {
            name if name.ends_with(".csv") => {
                Self::extract_from_xsense_csv(&attachment.content, &sensor_name)
            }
            name if name.ends_with(".json") => {
                Self::extract_from_json(&attachment.content)
            }
            name if name.ends_with(".xml") => {
                Self::extract_from_xml(&attachment.content)
            }
            name if name.ends_with(".txt") => {
                Self::extract_from_text(&attachment.content)
            }
            _ => {
                warn!("Unsupported file format: {}", attachment.filename);
                Ok(Vec::new())
            }
        }
    }
    
    fn extract_sensor_name(filename: &str) -> Result<String> {
        // Expected format: "Thermo-{sensor_name}_Export data_{date}.csv"
        // Examples: "Thermo-cabane_...", "Thermo-patio_...", "Thermo-poolhouse_..."
        
        if let Some(captures) = Regex::new(r"Thermo-([^_]+)_")?.captures(filename) {
            if let Some(sensor_match) = captures.get(1) {
                return Ok(sensor_match.as_str().to_string());
            }
        }
        
        // Fallback: use full filename without extension
        let name = filename
            .split('.')
            .next()
            .unwrap_or(filename);
        
        Ok(name.to_string())
    }
    
    fn extract_from_xsense_csv(content: &[u8], sensor_name: &str) -> Result<Vec<TemperatureReading>> {
        debug!("Extracting from X-Sense CSV file for sensor: {}", sensor_name);
        
        // Try UTF-8 first, then other encodings
        let content_str = match std::str::from_utf8(content) {
            Ok(s) => s.to_string(),
            Err(_) => {
                // Fallback: replace invalid characters with placeholders
                String::from_utf8_lossy(content).to_string()
            }
        };
        
        debug!("CSV content size: {} characters", content_str.len());
        
        let mut readings = Vec::new();
        let mut rdr = ReaderBuilder::new()
            .has_headers(true)
            .flexible(true)  // Tolerant to column differences
            .from_reader(content_str.as_bytes());
        
        // Check expected headers
        let headers = rdr.headers()
            .context("Unable to read CSV headers")?;
        debug!("Found CSV headers: {:?}", headers);
        
        // Validate we have at least 3 columns
        if headers.len() < 3 {
            return Err(anyhow::anyhow!("Invalid CSV: found {} columns, expected at least 3", headers.len()));
        }
        
        // Parse each data line
        for (line_num, result) in rdr.records().enumerate() {
            let record = result.context(format!("Error on line {}", line_num + 2))?;
            
            if record.len() < 3 {
                warn!("Line {} skipped: not enough columns ({} < 3)", line_num + 2, record.len());
                continue;
            }
            
            // Column 1: Timestamp (format: "2023/12/26 23:59")
            let timestamp_str = record.get(0).unwrap_or("");
            let timestamp = Self::parse_xsense_timestamp(timestamp_str)
                .with_context(|| format!("Unable to parse timestamp '{}' on line {}", timestamp_str, line_num + 2))?;
            
            // Column 2: Temperature (format: "5.5")
            let temperature_str = record.get(1).unwrap_or("");
            let temperature: f64 = temperature_str.parse()
                .with_context(|| format!("Unable to parse temperature '{}' on line {}", temperature_str, line_num + 2))?;
            
            // Column 3: Humidity (format: "89.6")
            let humidity_str = record.get(2).unwrap_or("");
            let humidity: f64 = humidity_str.parse()
                .with_context(|| format!("Unable to parse humidity '{}' on line {}", humidity_str, line_num + 2))?;
            
            readings.push(TemperatureReading {
                sensor_id: sensor_name.to_string(),
                timestamp,
                temperature,
                humidity: Some(humidity),
                location: Some(sensor_name.to_string()),
            });
        }
        
        info!("Extraction completed: {} temperature readings for sensor '{}'", readings.len(), sensor_name);
        Ok(readings)
    }
    
    fn parse_xsense_timestamp(timestamp_str: &str) -> Result<DateTime<Utc>> {
        // X-Sense format: "2023/12/26 23:59"
        let naive_dt = NaiveDateTime::parse_from_str(timestamp_str, "%Y/%m/%d %H:%M")
            .context(format!("Invalid timestamp format: '{}'", timestamp_str))?;
        
        // Convert to UTC (assuming data is in local time)
        Ok(DateTime::from_naive_utc_and_offset(naive_dt, Utc))
    }
    

    
    fn extract_from_json(content: &[u8]) -> Result<Vec<TemperatureReading>> {
        debug!("Extracting from JSON file");
        
        let content_str = std::str::from_utf8(content)
            .context("Unable to decode JSON content as UTF-8")?;
        
        // Try to deserialize directly as an array of readings
        if let Ok(readings) = serde_json::from_str::<Vec<TemperatureReading>>(content_str) {
            info!("Extracted {} temperature readings from JSON (direct format)", readings.len());
            return Ok(readings);
        }
        
        // Try other common JSON formats
        let value: serde_json::Value = serde_json::from_str(content_str)
            .context("Unable to parse JSON")?;
        
        let mut readings = Vec::new();
        
        // Format: {"data": [readings...]}
        if let Some(data_array) = value.get("data").and_then(|v| v.as_array()) {
            for item in data_array {
                if let Ok(reading) = Self::parse_json_reading(item) {
                    readings.push(reading);
                }
            }
        }
        // Format: {"readings": [readings...]}
        else if let Some(readings_array) = value.get("readings").and_then(|v| v.as_array()) {
            for item in readings_array {
                if let Ok(reading) = Self::parse_json_reading(item) {
                    readings.push(reading);
                }
            }
        }
        
        info!("Extracted {} temperature readings from JSON", readings.len());
        Ok(readings)
    }
    
    fn parse_json_reading(value: &serde_json::Value) -> Result<TemperatureReading> {
        let timestamp_str = value.get("timestamp")
            .or_else(|| value.get("time"))
            .or_else(|| value.get("date"))
            .and_then(|v| v.as_str())
            .context("Missing timestamp in JSON")?;
        
        let timestamp = Self::parse_timestamp(timestamp_str)?;
        
        let sensor_id = value.get("sensor_id")
            .or_else(|| value.get("sensor"))
            .or_else(|| value.get("device_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        
        let temperature = value.get("temperature")
            .or_else(|| value.get("temp"))
            .and_then(|v| v.as_f64())
            .context("Missing temperature in JSON")?;
        
        let humidity = value.get("humidity")
            .or_else(|| value.get("hum"))
            .and_then(|v| v.as_f64());
        
        let location = value.get("location")
            .or_else(|| value.get("room"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        
        Ok(TemperatureReading {
            sensor_id,
            timestamp,
            temperature,
            humidity,
            location,
        })
    }
    
    fn extract_from_xml(_content: &[u8]) -> Result<Vec<TemperatureReading>> {
        debug!("Extracting from XML file");
        // For now, return empty list
        // XML implementation to be added according to specific X-Sense format
        warn!("XML extraction not yet implemented");
        Ok(Vec::new())
    }
    
    fn extract_from_text(content: &[u8]) -> Result<Vec<TemperatureReading>> {
        debug!("Extracting from text file");
        
        let content_str = std::str::from_utf8(content)
            .context("Unable to decode text content as UTF-8")?;
        
        let mut readings = Vec::new();
        
        // Regex to capture common temperature patterns
        let temp_regex = Regex::new(r"(\d{4}-\d{2}-\d{2}[\sT]\d{2}:\d{2}:\d{2})[^\d]*(\w+)[^\d]*(-?\d+\.?\d*)[°C]*")?;
        
        for line in content_str.lines() {
            if let Some(captures) = temp_regex.captures(line) {
                if let (Some(timestamp_str), Some(sensor_id), Some(temp_str)) = 
                   (captures.get(1), captures.get(2), captures.get(3)) {
                    
                    if let (Ok(timestamp), Ok(temperature)) = 
                       (Self::parse_timestamp(timestamp_str.as_str()), temp_str.as_str().parse::<f64>()) {
                        
                        readings.push(TemperatureReading {
                            sensor_id: sensor_id.as_str().to_string(),
                            timestamp,
                            temperature,
                            humidity: None,
                            location: None,
                        });
                    }
                }
            }
        }
        
        info!("Extracted {} temperature readings from text file", readings.len());
        Ok(readings)
    }
    
    fn parse_timestamp(timestamp_str: &str) -> Result<DateTime<Utc>> {
        // Try different timestamp formats
        
        // ISO 8601 format with timezone
        if let Ok(dt) = DateTime::parse_from_rfc3339(timestamp_str) {
            return Ok(dt.with_timezone(&Utc));
        }
        
        // ISO 8601 format without timezone (assume UTC)
        if let Ok(naive_dt) = NaiveDateTime::parse_from_str(timestamp_str, "%Y-%m-%d %H:%M:%S") {
            return Ok(DateTime::from_naive_utc_and_offset(naive_dt, Utc));
        }
        
        // Format with T
        if let Ok(naive_dt) = NaiveDateTime::parse_from_str(timestamp_str, "%Y-%m-%dT%H:%M:%S") {
            return Ok(DateTime::from_naive_utc_and_offset(naive_dt, Utc));
        }
        
        // European format
        if let Ok(naive_dt) = NaiveDateTime::parse_from_str(timestamp_str, "%d/%m/%Y %H:%M:%S") {
            return Ok(DateTime::from_naive_utc_and_offset(naive_dt, Utc));
        }
        
        // American format
        if let Ok(naive_dt) = NaiveDateTime::parse_from_str(timestamp_str, "%m/%d/%Y %H:%M:%S") {
            return Ok(DateTime::from_naive_utc_and_offset(naive_dt, Utc));
        }
        
        anyhow::bail!("Unsupported timestamp format: {}", timestamp_str);
    }
}
