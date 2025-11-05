use anyhow::{Result, Context};
use log::{info, debug};
use std::path::PathBuf;
use std::fs;
use chrono::Utc;
use mail_parser::{MessageParser, MimeHeaders};
use base64::{Engine as _, engine::general_purpose};

#[derive(Debug)]
pub struct Attachment {
    pub filename: String,
    pub content: Vec<u8>,
    pub content_type: String,
}

pub struct AttachmentParser;

impl AttachmentParser {
    pub fn parse_email(raw_email: &[u8]) -> Result<Vec<Attachment>> {
        debug!("Parsing email pour extraire les pièces jointes");
        
        // For now, using a basic but functional MIME parser
        // We will improve this later with a better API
        let email_str = String::from_utf8_lossy(raw_email);
        let mut attachments = Vec::new();
        
        debug!("Email size: {} bytes", email_str.len());
        
        // First analyze the MIME structure of the email
        Self::analyze_email_structure(&email_str);
        
        // Rechercher les sections avec Content-Disposition: attachment
        let mut current_pos = 0;
        while let Some(attachment_start) = email_str[current_pos..].find("Content-Disposition: attachment") {
            let abs_start = current_pos + attachment_start;
            debug!("Found Content-Disposition: attachment at position {}", abs_start);
            
            // Search for filename
            if let Some(filename) = Self::extract_filename_from_headers(&email_str[abs_start..]) {
                debug!("Extracted filename: {}", filename);
                if Self::is_data_file(&filename) {
                    // Search for attachment content start
                    if let Some(content_start) = email_str[abs_start..].find("\r\n\r\n") {
                        let abs_content_start = abs_start + content_start + 4;
                        
                        // Search for attachment end (next boundary)
                        if let Some(content_end) = Self::find_attachment_end(&email_str[abs_content_start..]) {
                            let abs_content_end = abs_content_start + content_end;
                            let content_str = &email_str[abs_content_start..abs_content_end];
                            
                            debug!("Raw attachment content length: {} chars", content_str.len());
                            debug!("Raw content preview (first 200 chars): {}", 
                                   &content_str[..std::cmp::min(200, content_str.len())]);
                            
                            // Decode content (base64 or other)
                            if let Ok(content) = Self::decode_attachment_content(content_str) {
                                let content_type = Self::guess_content_type(&filename);
                                
                                debug!("Attachment found: {} ({}), taille: {} bytes", 
                                       filename, content_type, content.len());
                                
                                attachments.push(Attachment {
                                    filename,
                                    content,
                                    content_type,
                                });
                            }
                        }
                    }
                }
            }
            
            current_pos = abs_start + 1;
        }
        
        // If no attachment is found with manual approach,
        // try alternative approaches
        if attachments.is_empty() {
            debug!("No attachments found with manual parsing, trying alternative methods");
            Self::try_alternative_parsing(&email_str, &mut attachments)?;
        }
        
        info!("Found {} attachment(s)", attachments.len());
        Ok(attachments)
    }
    
    fn analyze_email_structure(email_str: &str) {
        debug!("=== ANALYSIS OF EMAIL STRUCTURE ===");
        
        // Search for boundaries
        let boundaries: Vec<_> = email_str.matches("boundary=").take(5).collect();
        debug!("Found {} boundary declarations", boundaries.len());
        
        // Search for Content-Type
        let content_types: Vec<_> = email_str.matches("Content-Type:").take(10).collect();
        debug!("Found {} Content-Type headers", content_types.len());
        
        // Search for Content-Disposition
        let dispositions: Vec<_> = email_str.matches("Content-Disposition:").take(10).collect();
        debug!("Found {} Content-Disposition headers", dispositions.len());
        
        // Display structure sample
        let lines: Vec<&str> = email_str.lines().collect();
        debug!("Email has {} lines total", lines.len());
        
        // Search for interesting patterns
        for (i, line) in lines.iter().enumerate().take(100) {
            if line.contains("Content-Type:") || 
               line.contains("Content-Disposition:") || 
               line.contains("boundary=") ||
               line.contains("filename=") {
                debug!("Line {}: {}", i + 1, line.trim());
            }
        }
        debug!("=== END EMAIL STRUCTURE ANALYSIS ===");
    }
    
    fn try_alternative_parsing(email_str: &str, attachments: &mut Vec<Attachment>) -> Result<()> {
        debug!("Trying alternative parsing methods");
        
        // Method 1: Search directly for "filename="
        Self::try_filename_direct_search(email_str, attachments)?;
        
        // Method 2: Use mail-parser as fallback
        if attachments.is_empty() {
            Self::try_mail_parser_fallback(email_str.as_bytes(), attachments)?;
        }
        
        Ok(())
    }
    
    fn try_filename_direct_search(email_str: &str, attachments: &mut Vec<Attachment>) -> Result<()> {
        debug!("Trying direct filename search");
        
        let mut current_pos = 0;
        while let Some(filename_start) = email_str[current_pos..].find("filename=") {
            let abs_start = current_pos + filename_start;
            let filename_line = &email_str[abs_start..abs_start + std::cmp::min(200, email_str.len() - abs_start)];
            
            if let Some(filename) = Self::extract_filename_from_line(filename_line) {
                debug!("Found filename via direct search: {}", filename);
                
                if Self::is_data_file(&filename) {
                    // Search for content associated with this filename
                    if let Some(content) = Self::find_content_for_filename(email_str, abs_start) {
                        let content_type = Self::guess_content_type(&filename);
                        
                        debug!("Found content for {}: {} bytes", filename, content.len());
                        
                        attachments.push(Attachment {
                            filename,
                            content,
                            content_type,
                        });
                    }
                }
            }
            
            current_pos = abs_start + 1;
        }
        
        Ok(())
    }
    
    fn extract_filename_from_line(line: &str) -> Option<String> {
        if let Some(start) = line.find("filename=") {
            let filename_part = &line[start + 9..];
            if let Some(end) = filename_part.find(['\r', '\n', ';', ' ']) {
                let filename = filename_part[..end].trim_matches('"').trim();
                if !filename.is_empty() {
                    return Some(filename.to_string());
                }
            } else {
                // Take rest of line
                let filename = filename_part.trim_matches('"').trim();
                if !filename.is_empty() {
                    return Some(filename.to_string());
                }
            }
        }
        None
    }
    
    fn find_content_for_filename(email_str: &str, filename_pos: usize) -> Option<Vec<u8>> {
        // Start from filename position and search for content
        let start_search = &email_str[filename_pos..];
        
        // Search for end of headers (double CRLF)
        if let Some(content_start_rel) = start_search.find("\r\n\r\n") {
            let content_start = filename_pos + content_start_rel + 4;
            
            // Search for content end (next boundary or email end)
            let content_search = &email_str[content_start..];
            let content_end = if let Some(boundary_pos) = content_search.find("\r\n--") {
                content_start + boundary_pos
            } else {
                email_str.len()
            };
            
            let content_str = &email_str[content_start..content_end];
            debug!("Found content block of {} chars for attachment", content_str.len());
            
            if let Ok(decoded) = Self::decode_attachment_content(content_str) {
                return Some(decoded);
            }
        }
        
        None
    }
    
    fn try_mail_parser_fallback(raw_email: &[u8], attachments: &mut Vec<Attachment>) -> Result<()> {
        debug!("Trying mail-parser fallback");
        
        if let Some(message) = MessageParser::default().parse(raw_email) {
            debug!("mail-parser successfully parsed the message");
            debug!("Message has {} attachments", message.attachments().count());
            
            for (i, part) in message.attachments().enumerate() {
                debug!("Processing attachment {}", i);
                
                
                // Try to extract real attachment content
                let contents = part.contents();
                debug!("Attachment {} content size: {} bytes", i, contents.len());
                
                // Essayer d'extraire le vrai nom du fichier
                let filename = Self::extract_real_filename_from_part(part, i);
                debug!("Extracted filename for attachment {}: {}", i, filename);
                
                // If content seems valid, use it
                if contents.len() > 10 {
                    let content_type = Self::guess_content_type(&filename);
                    
                    debug!("Using real attachment content: {} bytes", contents.len());
                    
                    attachments.push(Attachment {
                        filename,
                        content: contents.to_vec(),
                        content_type,
                    });
                } else {
                    debug!("Content too small for attachment {}, skipping", i);
                }
            }
        }
        
        Ok(())
    }
    
    fn extract_real_filename_from_part(part: &mail_parser::MessagePart, index: usize) -> String {
        // Try different methods to extract filename
        
        debug!("Trying to extract filename from attachment {}", index);
        
        
        let filename = part.attachment_name().unwrap().to_string();
        debug!("Generated filename: {}", filename);
        filename
    }    
    
    fn extract_filename_from_headers(headers: &str) -> Option<String> {
        // Chercher filename= dans les headers
        if let Some(filename_start) = headers.find("filename=") {
            let filename_part = &headers[filename_start + 9..];
            if let Some(filename_end) = filename_part.find(['\r', '\n', ';']) {
                let filename = filename_part[..filename_end].trim_matches('"').trim();
                if !filename.is_empty() {
                    return Some(filename.to_string());
                }
            }
        }
        None
    }
    
    fn find_attachment_end(content: &str) -> Option<usize> {
        // Search for next boundary or message end
        if let Some(boundary_pos) = content.find("--") {
            Some(boundary_pos)
        } else {
            Some(content.len())
        }
    }
    
    fn decode_attachment_content(content_str: &str) -> Result<Vec<u8>> {
        let content_str = content_str.trim();
        
        debug!("Attempting to decode content of {} chars", content_str.len());
        
        // Detect encoding type based on content
        if Self::is_base64_content(content_str) {
            debug!("Detected base64 encoding");
            if let Ok(decoded) = general_purpose::STANDARD.decode(content_str.replace(['\r', '\n', ' '], "")) {
                debug!("Successfully decoded {} bytes from base64", decoded.len());
                return Ok(decoded);
            } else {
                debug!("Base64 decoding failed, trying without whitespace removal");
                if let Ok(decoded) = general_purpose::STANDARD.decode(content_str) {
                    debug!("Successfully decoded {} bytes from base64 (second attempt)", decoded.len());
                    return Ok(decoded);
                }
            }
        } else if Self::is_quoted_printable_content(content_str) {
            debug!("Detected quoted-printable encoding");
            if let Ok(decoded) = Self::decode_quoted_printable(content_str) {
                debug!("Successfully decoded {} bytes from quoted-printable", decoded.len());
                return Ok(decoded);
            }
        } else {
            debug!("Content appears to be plain text");
        }
        
        // If no special decoding worked, treat as plain text
        let result = content_str.as_bytes().to_vec();
        debug!("Using content as raw bytes: {} bytes", result.len());
        Ok(result)
    }
    
    fn is_base64_content(content: &str) -> bool {
        // Base64 contains only A-Z, a-z, 0-9, +, /, = and whitespace characters
        let clean_content: String = content.chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        
        if clean_content.is_empty() {
            return false;
        }
        
        // Check that all characters are valid for base64
        let valid_chars = clean_content.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=');
        
        // And check there is a reasonable density of base64 characters
        let base64_ratio = clean_content.chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '+' || *c == '/')
            .count() as f64 / clean_content.len() as f64;
        
        valid_chars && base64_ratio > 0.8 && clean_content.len() > 10
    }
    
    fn is_quoted_printable_content(content: &str) -> bool {
        // Quoted-printable contains =XX sequences where XX are hex digits
        content.contains("=") && 
        content.chars().filter(|c| *c == '=').count() > 2
    }
    
    fn decode_quoted_printable(content: &str) -> Result<Vec<u8>> {
        let mut result = Vec::new();
        let mut chars = content.chars().peekable();
        
        while let Some(ch) = chars.next() {
            if ch == '=' {
                if let (Some(h1), Some(h2)) = (chars.next(), chars.next()) {
                    if let (Some(d1), Some(d2)) = (h1.to_digit(16), h2.to_digit(16)) {
                        result.push((d1 * 16 + d2) as u8);
                    } else {
                        // Malformed sequence, keep as is
                        result.push(ch as u8);
                        result.push(h1 as u8);
                        result.push(h2 as u8);
                    }
                } else {
                    // End of string, keep the =
                    result.push(ch as u8);
                }
            } else if ch == '\r' {
                // Skip CR in CRLF sequences
                if chars.peek() == Some(&'\n') {
                    chars.next(); // Skip the LF too
                }
            } else if ch != '\n' || !result.is_empty() {
                // Keep LF only if it's not at the beginning
                result.push(ch as u8);
            }
        }
        
        Ok(result)
    }
    
    fn guess_content_type(filename: &str) -> String {
        let lowercase_name = filename.to_lowercase();
        if lowercase_name.ends_with(".csv") {
            "text/csv".to_string()
        } else if lowercase_name.ends_with(".json") {
            "application/json".to_string()
        } else if lowercase_name.ends_with(".xml") {
            "application/xml".to_string()
        } else if lowercase_name.ends_with(".txt") {
            "text/plain".to_string()
        } else if lowercase_name.ends_with(".xlsx") {
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string()
        } else if lowercase_name.ends_with(".xls") {
            "application/vnd.ms-excel".to_string()
        } else {
            "application/octet-stream".to_string()
        }
    }
    

    
    fn is_data_file(filename: &str) -> bool {
        let lowercase_name = filename.to_lowercase();
        lowercase_name.ends_with(".csv") ||
        lowercase_name.ends_with(".json") ||
        lowercase_name.ends_with(".xml") ||
        lowercase_name.ends_with(".txt") ||
        lowercase_name.ends_with(".xlsx") ||
        lowercase_name.ends_with(".xls")
    }
    
    
    #[allow(dead_code)]
    pub fn save_attachment_to_data_dir_with_date(
        attachment: &Attachment, 
        data_dir: &str, 
        email_date: Option<chrono::DateTime<Utc>>
    ) -> Result<PathBuf> {
        // Create data directory if it does not exist
        fs::create_dir_all(data_dir)
            .context("Unable to create data directory")?;
        
        // Use email date if provided, otherwise current date
        let date_to_use = email_date.unwrap_or_else(Utc::now);
        let date_prefix = date_to_use.format("%Y%m%d_%H%M%S");
        let filename = format!("{}_{}", date_prefix, attachment.filename);
        let file_path = PathBuf::from(data_dir).join(&filename);
        
        // Save content
        fs::write(&file_path, &attachment.content)
            .context("Unable to write attachment")?;
        
        info!("Attachment saved: {:?}", file_path);
        
        Ok(file_path)
    }
    
    #[allow(dead_code)]
    pub fn display_attachment_info(attachment: &Attachment) {
        println!("📎 Attachment: {}", attachment.filename);
        println!("   Type: {}", attachment.content_type);
        println!("   Size: {} bytes", attachment.content.len());
        
        // Display content preview if text
        if attachment.content_type.starts_with("text/") || 
           attachment.filename.to_lowercase().ends_with(".csv") ||
           attachment.filename.to_lowercase().ends_with(".json") ||
           attachment.filename.to_lowercase().ends_with(".txt") {
            
            if let Ok(content_str) = std::str::from_utf8(&attachment.content) {
                let preview = if content_str.len() > 500 {
                    format!("{}...", &content_str[..500])
                } else {
                    content_str.to_string()
                };
                println!("   Preview:\n{}", preview);
            }
        }
        println!("   {}", "─".repeat(80));
    }
}