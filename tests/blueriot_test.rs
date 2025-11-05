use std::fs;
use homemetrics::blueriot::extractor;

#[test]
fn test_extract_from_blueriot_email() {
    // Load test email file
    let email_content = fs::read("data_test/blueriot.eml")
        .expect("Failed to read test email file data_test/blueriot.eml");
    
    // Parse email to extract text body
    let parsed_email = mail_parser::MessageParser::default()
        .parse(&email_content)
        .expect("Failed to parse email");
    
    // Extract text from email body
    let mut text_content = String::new();
    
    // Try to get text/plain body first
    if let Some(text_body) = parsed_email.body_text(0) {
        text_content.push_str(&text_body);
    }
    
    // If no text/plain, try text/html
    if text_content.is_empty() {
        if let Some(html_body) = parsed_email.body_html(0) {
            // Simple HTML stripping
            let html_str = html_body.to_string()
                .replace("<br>", "\n")
                .replace("<BR>", "\n")
                .replace("</p>", "\n")
                .replace("</P>", "\n");
            let tag_regex = regex::Regex::new(r"<[^>]+>").unwrap();
            text_content = tag_regex.replace_all(&html_str, "").to_string();
        }
    }
    
    assert!(!text_content.is_empty(), "No text content found in email");
    println!("📧 Email content extracted ({} chars)", text_content.len());
    
    // Extract pool metrics
    let email_date = chrono::Utc::now(); // Use current time for test
    let pool_reading = extractor::extract_pool_metrics(&text_content, email_date)
        .expect("Failed to extract pool metrics");
    
    println!("✅ Pool metrics extracted:");
    
    // Verify temperature
    if let Some(temp) = pool_reading.temperature {
        println!("   🌡️  Temperature: {:.1}°C", temp);
        assert!(temp > 0.0 && temp < 40.0, 
               "Temperature {} should be in reasonable pool range", temp);
    }
    
    // Verify pH
    if let Some(ph) = pool_reading.ph {
        println!("   🧪 pH: {:.2}", ph);
        assert!(ph > 0.0 && ph < 14.0, 
               "pH {} should be between 0 and 14", ph);
    }
    
    // Verify ORP
    if let Some(orp) = pool_reading.orp {
        println!("   ⚡ ORP: {} mV", orp);
        assert!(orp > -1000 && orp < 1000, 
               "ORP {} should be in reasonable range", orp);
    }
    
    // At least one metric should be present
    assert!(
        pool_reading.temperature.is_some() || 
        pool_reading.ph.is_some() || 
        pool_reading.orp.is_some(),
        "At least one pool metric should be extracted"
    );
}

#[test]
fn test_extract_pool_metrics_from_text() {
    let text_sample = r#"
        Bonjour,
        
        Voici les dernières mesures de votre piscine:
        
        Température: 15,8°C
        pH: 6,80
        ORP: 249 mV
        
        Cordialement,
        Blue Riot
    "#;
    
    let email_date = chrono::Utc::now();
    let pool_reading = extractor::extract_pool_metrics(text_sample, email_date)
        .expect("Failed to extract pool metrics from sample text");
    
    assert_eq!(pool_reading.temperature, Some(15.8));
    assert_eq!(pool_reading.ph, Some(6.80));
    assert_eq!(pool_reading.orp, Some(249));
    
    println!("✅ Text parsing test passed:");
    println!("   Temperature: {:?}°C", pool_reading.temperature);
    println!("   pH: {:?}", pool_reading.ph);
    println!("   ORP: {:?} mV", pool_reading.orp);
}

#[test]
fn test_extract_pool_metrics_various_formats() {
    // Test with dots as decimal separator
    let text1 = "Temperature: 18.5°C, pH: 7.2, ORP: 650 mV";
    let result1 = extractor::extract_pool_metrics(text1, chrono::Utc::now())
        .expect("Failed to parse format 1");
    assert_eq!(result1.temperature, Some(18.5));
    assert_eq!(result1.ph, Some(7.2));
    assert_eq!(result1.orp, Some(650));
    
    // Test with commas as decimal separator
    let text2 = "Température: 16,3°C\npH: 7,45\nORP: 550 mV";
    let result2 = extractor::extract_pool_metrics(text2, chrono::Utc::now())
        .expect("Failed to parse format 2");
    assert_eq!(result2.temperature, Some(16.3));
    assert_eq!(result2.ph, Some(7.45));
    assert_eq!(result2.orp, Some(550));
    
    println!("✅ Various format parsing tests passed");
}
