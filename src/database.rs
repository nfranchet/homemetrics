use anyhow::{Result, Context};
use log::{info, debug};
use sqlx::{PgPool, Row};

use crate::config::DatabaseConfig;
use crate::temperature_extractor::TemperatureReading;

pub struct Database {
    pool: PgPool,
}

impl Database {
    pub async fn new(config: &DatabaseConfig) -> Result<Self> {
        info!("Connexion à la base de données TimescaleDB");
        
        let database_url = format!(
            "postgres://{}:{}@{}:{}/{}",
            config.username, config.password, config.host, config.port, config.database
        );
        
        let pool = PgPool::connect(&database_url)
            .await
            .context("Impossible de se connecter à la base de données")?;
        
        info!("Connexion à la base de données établie");
        
        let db = Database { pool };
        
        // Créer les tables si elles n'existent pas
        db.create_tables_if_not_exists().await?;
        
        Ok(db)
    }
    
    async fn create_tables_if_not_exists(&self) -> Result<()> {
        info!("Vérification/création des tables de base de données");
        
        // Créer l'extension TimescaleDB si elle n'existe pas
        sqlx::query("CREATE EXTENSION IF NOT EXISTS timescaledb CASCADE")
            .execute(&self.pool)
            .await
            .context("Impossible de créer l'extension TimescaleDB")?;
        
        // Créer la table des capteurs
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS sensors (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                sensor_id VARCHAR(255) UNIQUE NOT NULL,
                location VARCHAR(255),
                created_at TIMESTAMPTZ DEFAULT NOW(),
                updated_at TIMESTAMPTZ DEFAULT NOW()
            )
            "#
        )
        .execute(&self.pool)
        .await
        .context("Impossible de créer la table sensors")?;
        
        // Créer la table des lectures de température
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS temperature_readings (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                sensor_id VARCHAR(255) NOT NULL,
                timestamp TIMESTAMPTZ NOT NULL,
                temperature DOUBLE PRECISION NOT NULL,
                humidity DOUBLE PRECISION,
                location VARCHAR(255),
                processed_at TIMESTAMPTZ DEFAULT NOW(),
                FOREIGN KEY (sensor_id) REFERENCES sensors(sensor_id) ON DELETE CASCADE
            )
            "#
        )
        .execute(&self.pool)
        .await
        .context("Impossible de créer la table temperature_readings")?;
        
        // Créer une hypertable TimescaleDB pour les lectures de température
        let _result = sqlx::query(
            "SELECT create_hypertable('temperature_readings', 'timestamp', if_not_exists => TRUE)"
        )
        .execute(&self.pool)
        .await;
        // Ignorer l'erreur si la hypertable existe déjà
        
        // Créer des index pour optimiser les requêtes
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_temp_readings_sensor_time ON temperature_readings (sensor_id, timestamp DESC)"
        )
        .execute(&self.pool)
        .await
        .context("Impossible de créer l'index sur sensor_id et timestamp")?;
        
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_temp_readings_timestamp ON temperature_readings (timestamp DESC)"
        )
        .execute(&self.pool)
        .await
        .context("Impossible de créer l'index sur timestamp")?;
        
        info!("Tables de base de données vérifiées/créées avec succès");
        Ok(())
    }
    
    pub async fn save_temperature_readings(&self, readings: &[TemperatureReading]) -> Result<usize> {
        if readings.is_empty() {
            return Ok(0);
        }
        
        info!("Sauvegarde de {} lectures de température", readings.len());
        
        let mut transaction = self.pool.begin()
            .await
            .context("Impossible de commencer la transaction")?;
        
        let mut saved_count = 0;
        
        for reading in readings {
            // D'abord, s'assurer que le capteur existe
            self.ensure_sensor_exists(&mut transaction, &reading.sensor_id, &reading.location).await?;
            
            // Vérifier si cette lecture existe déjà (éviter les doublons)
            let exists = sqlx::query(
                "SELECT 1 FROM temperature_readings WHERE sensor_id = $1 AND timestamp = $2"
            )
            .bind(&reading.sensor_id)
            .bind(&reading.timestamp)
            .fetch_optional(&mut *transaction)
            .await
            .context("Erreur lors de la vérification des doublons")?;
            
            if exists.is_some() {
                debug!("Lecture déjà existante ignorée: {} à {}", reading.sensor_id, reading.timestamp);
                continue;
            }
            
            // Insérer la nouvelle lecture
            sqlx::query(
                r#"
                INSERT INTO temperature_readings 
                (sensor_id, timestamp, temperature, humidity, location)
                VALUES ($1, $2, $3, $4, $5)
                "#
            )
            .bind(&reading.sensor_id)
            .bind(&reading.timestamp)
            .bind(reading.temperature)
            .bind(reading.humidity)
            .bind(&reading.location)
            .execute(&mut *transaction)
            .await
            .context("Erreur lors de l'insertion de la lecture de température")?;
            
            saved_count += 1;
            
            debug!("Lecture sauvegardée: {} = {}°C à {}", 
                   reading.sensor_id, reading.temperature, reading.timestamp);
        }
        
        transaction.commit()
            .await
            .context("Erreur lors de la validation de la transaction")?;
        
        info!("Sauvegarde terminée: {} nouvelles lectures sur {} traitées", saved_count, readings.len());
        Ok(saved_count)
    }
    
    async fn ensure_sensor_exists(
        &self, 
        transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        sensor_id: &str,
        location: &Option<String>
    ) -> Result<()> {
        let exists = sqlx::query("SELECT 1 FROM sensors WHERE sensor_id = $1")
            .bind(sensor_id)
            .fetch_optional(&mut **transaction)
            .await
            .context("Erreur lors de la vérification de l'existence du capteur")?;
        
        if exists.is_none() {
            sqlx::query(
                "INSERT INTO sensors (sensor_id, location) VALUES ($1, $2)"
            )
            .bind(sensor_id)
            .bind(location)
            .execute(&mut **transaction)
            .await
            .context("Erreur lors de l'insertion du capteur")?;
            
            debug!("Nouveau capteur créé: {}", sensor_id);
        } else if let Some(loc) = location {
            // Mettre à jour la localisation si elle est fournie
            sqlx::query(
                "UPDATE sensors SET location = $2, updated_at = NOW() WHERE sensor_id = $1 AND location IS NULL"
            )
            .bind(sensor_id)
            .bind(loc)
            .execute(&mut **transaction)
            .await
            .context("Erreur lors de la mise à jour de la localisation du capteur")?;
        }
        
        Ok(())
    }
    
    pub async fn get_latest_readings(&self, sensor_id: Option<&str>, limit: i64) -> Result<Vec<TemperatureReading>> {
        let query = if let Some(sid) = sensor_id {
            sqlx::query(
                r#"
                SELECT sensor_id, timestamp, temperature, humidity, location
                FROM temperature_readings 
                WHERE sensor_id = $1
                ORDER BY timestamp DESC 
                LIMIT $2
                "#
            )
            .bind(sid)
            .bind(limit)
        } else {
            sqlx::query(
                r#"
                SELECT sensor_id, timestamp, temperature, humidity, location
                FROM temperature_readings 
                ORDER BY timestamp DESC 
                LIMIT $1
                "#
            )
            .bind(limit)
        };
        
        let rows = query.fetch_all(&self.pool)
            .await
            .context("Erreur lors de la récupération des lectures")?;
        
        let mut readings = Vec::new();
        for row in rows {
            readings.push(TemperatureReading {
                sensor_id: row.get("sensor_id"),
                timestamp: row.get("timestamp"),
                temperature: row.get("temperature"),
                humidity: row.get("humidity"),
                location: row.get("location"),
            });
        }
        
        Ok(readings)
    }
    
    pub async fn close(self) -> Result<()> {
        info!("Fermeture de la connexion à la base de données");
        self.pool.close().await;
        Ok(())
    }
}