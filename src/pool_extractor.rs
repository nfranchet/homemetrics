use anyhow::Result;
use chrono::{DateTime, Utc};
use log::{debug, warn};

#[derive(Debug, Clone)]
pub struct PoolReading {
    pub timestamp: DateTime<Utc>,
    pub temperature: Option<f64>,
    pub ph: Option<f64>,
    pub orp: Option<i32>,
}

/// Extract pool metrics from Blue Riot email text content
/// 
/// Expected formats:
/// - "Temperature: 25.5°C" or "Température: 25.5°C" or "Temp: 25.5"
/// - "pH: 7.2" or "pH: 7,2"
/// - "ORP: 720 mV" or "Redox: 720" or "ORP: 720mV"
pub fn extract_pool_metrics(text: &str, timestamp: DateTime<Utc>) -> Result<PoolReading> {
    debug!("Extracting pool metrics from text (length: {} bytes)", text.len());
    
    let mut reading = PoolReading {
        timestamp,
        temperature: None,
        ph: None,
        orp: None,
    };
    
    // Extract temperature
    reading.temperature = extract_temperature(text);
    
    // Extract pH
    reading.ph = extract_ph(text);
    
    // Extract ORP
    reading.orp = extract_orp(text);
    
    // Validate that we extracted at least one metric
    if reading.temperature.is_none() && reading.ph.is_none() && reading.orp.is_none() {
        anyhow::bail!("No pool metrics found in email text");
    }
    
    debug!("Extracted pool reading: temp={:?}°C, pH={:?}, ORP={:?} mV", 
           reading.temperature, reading.ph, reading.orp);
    
    Ok(reading)
}

/// Extract temperature from text
/// Patterns: "Temperature: 25.5°C", "Température: 25.5", "Temp: 25,5"
fn extract_temperature(text: &str) -> Option<f64> {
    // Try different patterns
    let patterns = [
        r"(?i)temp[ée]rature[:\s]+([0-9]+[.,][0-9]+)",
        r"(?i)temp[:\s]+([0-9]+[.,][0-9]+)",
        r"([0-9]+[.,][0-9]+)\s*°C",
    ];
    
    for pattern in &patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            if let Some(caps) = re.captures(text) {
                if let Some(temp_str) = caps.get(1) {
                    let temp_normalized = temp_str.as_str().replace(',', ".");
                    if let Ok(temp) = temp_normalized.parse::<f64>() {
                        debug!("Found temperature: {}°C (pattern: {})", temp, pattern);
                        return Some(temp);
                    }
                }
            }
        }
    }
    
    warn!("Temperature not found in text");
    None
}

/// Extract pH from text
/// Patterns: "pH: 7.2", "pH 7,2", "ph: 7.25"
fn extract_ph(text: &str) -> Option<f64> {
    let patterns = [
        r"(?i)ph[:\s]+([0-9]+[.,][0-9]+)",
        r"(?i)ph\s*=\s*([0-9]+[.,][0-9]+)",
    ];
    
    for pattern in &patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            if let Some(caps) = re.captures(text) {
                if let Some(ph_str) = caps.get(1) {
                    let ph_normalized = ph_str.as_str().replace(',', ".");
                    if let Ok(ph) = ph_normalized.parse::<f64>() {
                        // Validate pH range (0-14)
                        if (0.0..=14.0).contains(&ph) {
                            debug!("Found pH: {} (pattern: {})", ph, pattern);
                            return Some(ph);
                        } else {
                            warn!("pH value out of range: {}", ph);
                        }
                    }
                }
            }
        }
    }
    
    warn!("pH not found in text");
    None
}

/// Extract ORP (Oxidation-Reduction Potential) from text
/// Patterns: "ORP: 720 mV", "Redox: 720", "ORP: 720mV"
fn extract_orp(text: &str) -> Option<i32> {
    let patterns = [
        r"(?i)orp[:\s]+([0-9]+)\s*m?V?",
        r"(?i)redox[:\s]+([0-9]+)\s*m?V?",
        r"([0-9]+)\s*mV",
    ];
    
    for pattern in &patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            if let Some(caps) = re.captures(text) {
                if let Some(orp_str) = caps.get(1) {
                    if let Ok(orp) = orp_str.as_str().parse::<i32>() {
                        // Validate ORP range (typically 0-1000 mV for pools)
                        if (0..=1000).contains(&orp) {
                            debug!("Found ORP: {} mV (pattern: {})", orp, pattern);
                            return Some(orp);
                        } else {
                            warn!("ORP value out of range: {}", orp);
                        }
                    }
                }
            }
        }
    }
    
    warn!("ORP not found in text");
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_extract_temperature() {
        assert_eq!(extract_temperature("Temperature: 25.5°C"), Some(25.5));
        assert_eq!(extract_temperature("Température: 24,8°C"), Some(24.8));
        assert_eq!(extract_temperature("Temp: 26.2"), Some(26.2));
        assert_eq!(extract_temperature("No temp here"), None);
    }
    
    #[test]
    fn test_extract_ph() {
        assert_eq!(extract_ph("pH: 7.2"), Some(7.2));
        assert_eq!(extract_ph("pH 7,4"), Some(7.4));
        assert_eq!(extract_ph("ph = 7.15"), Some(7.15));
        assert_eq!(extract_ph("No pH here"), None);
        assert_eq!(extract_ph("pH: 15.0"), None); // Out of range
    }
    
    #[test]
    fn test_extract_orp() {
        assert_eq!(extract_orp("ORP: 720 mV"), Some(720));
        assert_eq!(extract_orp("Redox: 680"), Some(680));
        assert_eq!(extract_orp("ORP: 750mV"), Some(750));
        assert_eq!(extract_orp("No ORP here"), None);
        assert_eq!(extract_orp("ORP: 2000"), None); // Out of range
    }
    
    #[test]
    fn test_extract_pool_metrics() {
        let text = "Pool Status Report\nTemperature: 25.5°C\npH: 7.2\nORP: 720 mV";
        let timestamp = Utc::now();
        let reading = extract_pool_metrics(text, timestamp).unwrap();
        
        assert_eq!(reading.temperature, Some(25.5));
        assert_eq!(reading.ph, Some(7.2));
        assert_eq!(reading.orp, Some(720));
    }
}
