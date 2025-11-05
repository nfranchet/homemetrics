use std::fs;
use homemetrics::attachment_parser::AttachmentParser;
use homemetrics::xsense::TemperatureExtractor;

#[test]
fn test_extract_from_xsense_email() {
    // Load test email file
    let email_content = fs::read("data_test/xsense.eml")
        .expect("Failed to read test email file data_test/xsense.eml");
    
    // Parse email to extract attachments
    let attachments = AttachmentParser::parse_email(&email_content)
        .expect("Failed to parse email");
    
    assert!(!attachments.is_empty(), "No attachments found in test email");
    println!("📎 Found {} attachment(s)", attachments.len());
    
    // Extract temperature data from first attachment
    let readings = TemperatureExtractor::extract_from_attachment(&attachments[0])
        .expect("Failed to extract temperature readings");
    
    assert!(!readings.is_empty(), "No temperature readings extracted");
    
    // Verify data structure
    for reading in &readings {
        assert!(!reading.sensor_id.is_empty(), "Sensor ID should not be empty");
        assert!(reading.temperature > -50.0 && reading.temperature < 50.0, 
               "Temperature {} should be in reasonable range", reading.temperature);
        if let Some(humidity) = reading.humidity {
            assert!(humidity >= 0.0 && humidity <= 100.0, 
                   "Humidity {} should be between 0 and 100", humidity);
        }
    }
    
    println!("✅ Extracted {} temperature readings from X-Sense email", readings.len());
    println!("   Sensor ID: {}", readings[0].sensor_id);
    println!("   First reading: {:.1}°C at {}", 
             readings[0].temperature, 
             readings[0].timestamp);
    if let Some(humidity) = readings[0].humidity {
        println!("   Humidity: {:.1}%", humidity);
    }
}

#[test]
fn test_extract_sensor_name() {
    // Test extracting sensor name from actual X-Sense filename format
    assert_eq!(
        TemperatureExtractor::extract_sensor_name("Thermo-cabane_Exporter les données_20251104.csv").unwrap(),
        "cabane"
    );
    
    assert_eq!(
        TemperatureExtractor::extract_sensor_name("Thermo-patio_Exporter les données_20251105.csv").unwrap(),
        "patio"
    );
}

#[test]
fn test_csv_parsing() {
    // Test CSV parsing with actual X-Sense CSV format
    let csv_content = b"Temps,Temp\xC3\xA9rature_Celsius,Humidit\xC3\xA9 relative_Pourcentage
2025/11/04 23:59,15.0,84.0
2025/11/04 23:58,15.1,83.2
2025/11/04 23:57,15.2,84.0";
    
    let readings = TemperatureExtractor::extract_from_xsense_csv(csv_content, "TEST_SENSOR")
        .expect("Failed to parse CSV");
    
    assert_eq!(readings.len(), 3);
    
    // Check first reading
    assert_eq!(readings[0].sensor_id, "TEST_SENSOR");
    assert_eq!(readings[0].temperature, 15.0);
    assert_eq!(readings[0].humidity, Some(84.0));
    
    println!("✅ CSV parsing test passed:");
    println!("   Parsed {} readings", readings.len());
    println!("   First reading: {}°C, {}% humidity", 
             readings[0].temperature, 
             readings[0].humidity.unwrap());
}
