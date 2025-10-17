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
        info!("Extraction des données de température depuis: {}", attachment.filename);
        
        // Extraire le nom du sensor depuis le nom de fichier
        let sensor_name = Self::extract_sensor_name(&attachment.filename)?;
        debug!("Nom du sensor extrait: {}", sensor_name);
        
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
                warn!("Format de fichier non supporté: {}", attachment.filename);
                Ok(Vec::new())
            }
        }
    }
    
    fn extract_sensor_name(filename: &str) -> Result<String> {
        // Format attendu: "Thermo-{sensor_name}_Exporter les données_{date}.csv"
        // Exemples: "Thermo-cabane_...", "Thermo-patio_...", "Thermo-poolhouse_..."
        
        if let Some(captures) = Regex::new(r"Thermo-([^_]+)_")?.captures(filename) {
            if let Some(sensor_match) = captures.get(1) {
                return Ok(sensor_match.as_str().to_string());
            }
        }
        
        // Fallback: utiliser le nom complet du fichier sans extension
        let name = filename
            .split('.')
            .next()
            .unwrap_or(filename);
        
        Ok(name.to_string())
    }
    
    fn extract_from_xsense_csv(content: &[u8], sensor_name: &str) -> Result<Vec<TemperatureReading>> {
        debug!("Extraction depuis fichier CSV X-Sense pour le sensor: {}", sensor_name);
        
        // Essayer d'abord UTF-8, puis d'autres encodages
        let content_str = match std::str::from_utf8(content) {
            Ok(s) => s.to_string(),
            Err(_) => {
                // Fallback: remplacer les caractères invalides par des placeholders
                String::from_utf8_lossy(content).to_string()
            }
        };
        
        debug!("Taille du contenu CSV: {} caractères", content_str.len());
        
        let mut readings = Vec::new();
        let mut rdr = ReaderBuilder::new()
            .has_headers(true)
            .flexible(true)  // Tolérant aux différences de colonnes
            .from_reader(content_str.as_bytes());
        
        // Vérifier les headers attendus
        let headers = rdr.headers()
            .context("Impossible de lire les headers CSV")?;
        debug!("Headers CSV trouvés: {:?}", headers);
        
        // Valider qu'on a au moins 3 colonnes
        if headers.len() < 3 {
            return Err(anyhow::anyhow!("CSV invalide: trouvé {} colonnes, attendu au moins 3", headers.len()));
        }
        
        // Parser chaque ligne de données
        for (line_num, result) in rdr.records().enumerate() {
            let record = result.context(format!("Erreur ligne {}", line_num + 2))?;
            
            if record.len() < 3 {
                warn!("Ligne {} ignorée: pas assez de colonnes ({} < 3)", line_num + 2, record.len());
                continue;
            }
            
            // Colonne 1: Timestamp (format: "2023/12/26 23:59")
            let timestamp_str = record.get(0).unwrap_or("");
            let timestamp = Self::parse_xsense_timestamp(timestamp_str)
                .with_context(|| format!("Impossible de parser le timestamp '{}' ligne {}", timestamp_str, line_num + 2))?;
            
            // Colonne 2: Température (format: "5.5")
            let temperature_str = record.get(1).unwrap_or("");
            let temperature: f64 = temperature_str.parse()
                .with_context(|| format!("Impossible de parser la température '{}' ligne {}", temperature_str, line_num + 2))?;
            
            // Colonne 3: Humidité (format: "89.6")
            let humidity_str = record.get(2).unwrap_or("");
            let humidity: f64 = humidity_str.parse()
                .with_context(|| format!("Impossible de parser l'humidité '{}' ligne {}", humidity_str, line_num + 2))?;
            
            readings.push(TemperatureReading {
                sensor_id: sensor_name.to_string(),
                timestamp,
                temperature,
                humidity: Some(humidity),
                location: Some(sensor_name.to_string()),
            });
        }
        
        info!("Extraction terminée: {} lectures de température pour le sensor '{}'", readings.len(), sensor_name);
        Ok(readings)
    }
    
    fn parse_xsense_timestamp(timestamp_str: &str) -> Result<DateTime<Utc>> {
        // Format X-Sense: "2023/12/26 23:59"
        let naive_dt = NaiveDateTime::parse_from_str(timestamp_str, "%Y/%m/%d %H:%M")
            .context(format!("Format timestamp invalide: '{}'", timestamp_str))?;
        
        // Convertir en UTC (on suppose que les données sont en heure locale)
        Ok(DateTime::from_naive_utc_and_offset(naive_dt, Utc))
    }
    
    fn extract_from_csv(content: &[u8]) -> Result<Vec<TemperatureReading>> {
        debug!("Extraction depuis fichier CSV générique");
        
        let content_str = std::str::from_utf8(content)
            .context("Impossible de décoder le contenu CSV en UTF-8")?;
        
        let mut reader = ReaderBuilder::new()
            .has_headers(true)
            .from_reader(content_str.as_bytes());
        
        let mut readings = Vec::new();
        
        // Essayer différents formats de CSV courants pour les capteurs X-Sense
        for result in reader.records() {
            let record = result.context("Erreur lors de la lecture d'un enregistrement CSV")?;
            
            if let Ok(reading) = Self::parse_csv_record(&record) {
                readings.push(reading);
            }
        }
        
        info!("Extrait {} lectures de température depuis CSV", readings.len());
        Ok(readings)
    }
    
    fn parse_csv_record(record: &csv::StringRecord) -> Result<TemperatureReading> {
        // Format attendu: timestamp, sensor_id, temperature, [humidity], [location]
        // Ou variations communes
        
        if record.len() < 3 {
            anyhow::bail!("Enregistrement CSV insuffisant: {} colonnes", record.len());
        }
        
        // Analyser timestamp (colonne 0 ou 1)
        let timestamp_str = record.get(0).unwrap_or("");
        let timestamp = Self::parse_timestamp(timestamp_str)?;
        
        // Analyser sensor_id
        let sensor_id = record.get(1).unwrap_or("unknown").to_string();
        
        // Analyser température
        let temp_str = record.get(2).unwrap_or("0");
        let temperature: f64 = temp_str.parse()
            .context("Impossible de parser la température")?;
        
        // Analyser humidité (optionnel)
        let humidity = if record.len() > 3 {
            record.get(3).and_then(|s| s.parse().ok())
        } else {
            None
        };
        
        // Analyser localisation (optionnel)
        let location = if record.len() > 4 {
            record.get(4).map(|s| s.to_string()).filter(|s| !s.is_empty())
        } else {
            None
        };
        
        Ok(TemperatureReading {
            sensor_id,
            timestamp,
            temperature,
            humidity,
            location,
        })
    }
    
    fn extract_from_json(content: &[u8]) -> Result<Vec<TemperatureReading>> {
        debug!("Extraction depuis fichier JSON");
        
        let content_str = std::str::from_utf8(content)
            .context("Impossible de décoder le contenu JSON en UTF-8")?;
        
        // Essayer de désérialiser directement comme un tableau de lectures
        if let Ok(readings) = serde_json::from_str::<Vec<TemperatureReading>>(content_str) {
            info!("Extrait {} lectures de température depuis JSON (format direct)", readings.len());
            return Ok(readings);
        }
        
        // Essayer d'autres formats JSON courants
        let value: serde_json::Value = serde_json::from_str(content_str)
            .context("Impossible de parser le JSON")?;
        
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
        
        info!("Extrait {} lectures de température depuis JSON", readings.len());
        Ok(readings)
    }
    
    fn parse_json_reading(value: &serde_json::Value) -> Result<TemperatureReading> {
        let timestamp_str = value.get("timestamp")
            .or_else(|| value.get("time"))
            .or_else(|| value.get("date"))
            .and_then(|v| v.as_str())
            .context("Timestamp manquant dans le JSON")?;
        
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
            .context("Température manquante dans le JSON")?;
        
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
        debug!("Extraction depuis fichier XML");
        // Pour l'instant, retourner une liste vide
        // Implémentation XML à ajouter selon le format spécifique X-Sense
        warn!("Extraction XML non encore implémentée");
        Ok(Vec::new())
    }
    
    fn extract_from_text(content: &[u8]) -> Result<Vec<TemperatureReading>> {
        debug!("Extraction depuis fichier texte");
        
        let content_str = std::str::from_utf8(content)
            .context("Impossible de décoder le contenu texte en UTF-8")?;
        
        let mut readings = Vec::new();
        
        // Regex pour capturer les patterns de température courants
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
        
        info!("Extrait {} lectures de température depuis fichier texte", readings.len());
        Ok(readings)
    }
    
    fn parse_timestamp(timestamp_str: &str) -> Result<DateTime<Utc>> {
        // Essayer différents formats de timestamp
        
        // Format ISO 8601 avec timezone
        if let Ok(dt) = DateTime::parse_from_rfc3339(timestamp_str) {
            return Ok(dt.with_timezone(&Utc));
        }
        
        // Format ISO 8601 sans timezone (assumer UTC)
        if let Ok(naive_dt) = NaiveDateTime::parse_from_str(timestamp_str, "%Y-%m-%d %H:%M:%S") {
            return Ok(DateTime::from_naive_utc_and_offset(naive_dt, Utc));
        }
        
        // Format avec T
        if let Ok(naive_dt) = NaiveDateTime::parse_from_str(timestamp_str, "%Y-%m-%dT%H:%M:%S") {
            return Ok(DateTime::from_naive_utc_and_offset(naive_dt, Utc));
        }
        
        // Format européen
        if let Ok(naive_dt) = NaiveDateTime::parse_from_str(timestamp_str, "%d/%m/%Y %H:%M:%S") {
            return Ok(DateTime::from_naive_utc_and_offset(naive_dt, Utc));
        }
        
        // Format américain
        if let Ok(naive_dt) = NaiveDateTime::parse_from_str(timestamp_str, "%m/%d/%Y %H:%M:%S") {
            return Ok(DateTime::from_naive_utc_and_offset(naive_dt, Utc));
        }
        
        anyhow::bail!("Format de timestamp non supporté: {}", timestamp_str);
    }
}