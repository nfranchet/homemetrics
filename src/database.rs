use anyhow::{Result, Context};
use log::{info, debug, warn};
use sqlx::{PgPool, Row};

use crate::config::DatabaseConfig;
use crate::xsense::TemperatureReading;
use crate::blueriot::PoolReading;

pub struct Database {
    pool: PgPool,
}

impl Database {
    pub async fn new(config: &DatabaseConfig) -> Result<Self> {
        info!("Connecting to TimescaleDB database");
        
        let database_url = format!(
            "postgres://{}:{}@{}:{}/{}",
            config.username, config.password, config.host, config.port, config.database
        );
        
        let pool = PgPool::connect(&database_url)
            .await
            .context("Impossible de se connecter à la base de données")?;
        
        info!("Database connection established");
        
        let db = Database { pool };
        
        // Create tables if they don't exist
        db.create_tables_if_not_exists().await?;
        
        Ok(db)
    }
    
    async fn create_tables_if_not_exists(&self) -> Result<()> {
        info!("Checking/creating database tables");
        
        // Try to create TimescaleDB extension if available
        let _timescaledb_available = match sqlx::query("CREATE EXTENSION IF NOT EXISTS timescaledb CASCADE")
            .execute(&self.pool)
            .await {
                Ok(_) => {
                    info!("✅ TimescaleDB extension created/available");
                    true
                },
                Err(e) => {
                    warn!("⚠️  TimescaleDB not available, using standard PostgreSQL: {}", e);
                    false
                }
            };
        
        // Create sensors table
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
        .context("Unable to create sensors table")?;
        
        // Create temperature readings table
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
        .context("Unable to create temperature_readings table")?;
        
        // Create TimescaleDB hypertable for temperature readings
        let _result = sqlx::query(
            "SELECT create_hypertable('temperature_readings', 'timestamp', if_not_exists => TRUE)"
        )
        .execute(&self.pool)
        .await;
        // Ignore error if hypertable already exists
        
        // Create indexes to optimize queries
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_temp_readings_sensor_time ON temperature_readings (sensor_id, timestamp DESC)"
        )
        .execute(&self.pool)
        .await
        .context("Unable to create index sur sensor_id et timestamp")?;
        
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_temp_readings_timestamp ON temperature_readings (timestamp DESC)"
        )
        .execute(&self.pool)
        .await
        .context("Unable to create index sur timestamp")?;
        
        // Create pool_readings table for Blue Riot pool monitoring
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS pool_readings (
                id SERIAL,
                timestamp TIMESTAMPTZ NOT NULL,
                temperature NUMERIC(5,2),
                ph NUMERIC(4,2),
                orp INTEGER,
                email_id VARCHAR(255),
                created_at TIMESTAMPTZ DEFAULT NOW(),
                PRIMARY KEY (id, timestamp)
            )
            "#
        )
        .execute(&self.pool)
        .await
        .context("Unable to create pool_readings table")?;
        
        // Create TimescaleDB hypertable for pool readings
        let _result = sqlx::query(
            "SELECT create_hypertable('pool_readings', 'timestamp', if_not_exists => TRUE)"
        )
        .execute(&self.pool)
        .await;
        // Ignore error if hypertable already exists
        
        // Create indexes for pool readings
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_pool_readings_timestamp ON pool_readings (timestamp DESC)"
        )
        .execute(&self.pool)
        .await
        .context("Unable to create index on pool_readings timestamp")?;
        
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_pool_readings_email ON pool_readings (email_id)"
        )
        .execute(&self.pool)
        .await
        .context("Unable to create index on pool_readings email_id")?;
        
        info!("Database tables checked/created successfully");
        Ok(())
    }
    
    pub async fn save_temperature_readings(&self, readings: &[TemperatureReading]) -> Result<usize> {
        if readings.is_empty() {
            return Ok(0);
        }
        
        info!("Saving {} temperature readings", readings.len());
        
        let mut transaction = self.pool.begin()
            .await
            .context("Unable to start transaction")?;
        
        let mut saved_count = 0;
        
        for reading in readings {
            // First, make sure the sensor exists
            self.ensure_sensor_exists(&mut transaction, &reading.sensor_id, &reading.location).await?;
            
            // Check if this reading already exists (avoid duplicates)
            let exists = sqlx::query(
                "SELECT 1 FROM temperature_readings WHERE sensor_id = $1 AND timestamp = $2"
            )
            .bind(&reading.sensor_id)
            .bind(reading.timestamp)
            .fetch_optional(&mut *transaction)
            .await
            .context("Error checking for duplicates")?;
            
            if exists.is_some() {
                saved_count += 1; // We count duplicates as "saved" to reflect total processed
                debug!("Existing reading skipped: {} à {}", reading.sensor_id, reading.timestamp);
                continue;
            }
            
            // Insert new reading
            sqlx::query(
                r#"
                INSERT INTO temperature_readings 
                (sensor_id, timestamp, temperature, humidity, location)
                VALUES ($1, $2, $3, $4, $5)
                "#
            )
            .bind(&reading.sensor_id)
            .bind(reading.timestamp)
            .bind(reading.temperature)
            .bind(reading.humidity)
            .bind(&reading.location)
            .execute(&mut *transaction)
            .await
            .context("Error inserting temperature reading")?;
            
            saved_count += 1;
            
            debug!("Reading saved: {} = {}°C à {}", 
                   reading.sensor_id, reading.temperature, reading.timestamp);
        }
        
        transaction.commit()
            .await
            .context("Error committing transaction")?;
        
        info!("Save completed: {} new readings out of {} processed", saved_count, readings.len());
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
            .context("Error checking sensor existence")?;
        
        if exists.is_none() {
            sqlx::query(
                "INSERT INTO sensors (sensor_id, location) VALUES ($1, $2)"
            )
            .bind(sensor_id)
            .bind(location)
            .execute(&mut **transaction)
            .await
            .context("Error inserting sensor")?;
            
            debug!("New sensor created: {}", sensor_id);
        } else if let Some(loc) = location {
            // Update location if provided
            sqlx::query(
                "UPDATE sensors SET location = $2, updated_at = NOW() WHERE sensor_id = $1 AND location IS NULL"
            )
            .bind(sensor_id)
            .bind(loc)
            .execute(&mut **transaction)
            .await
            .context("Error updating sensor location")?;
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
            .context("Error retrieving readings")?;
        
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
    
    /// Save a pool reading to the database
    pub async fn save_pool_reading(&self, reading: &PoolReading, email_id: &str) -> Result<()> {
        debug!("Saving pool reading: temp={:?}°C, pH={:?}, ORP={:?} mV", 
               reading.temperature, reading.ph, reading.orp);
        
        // Check if this reading already exists (by email_id)
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM pool_readings WHERE email_id = $1)"
        )
        .bind(email_id)
        .fetch_one(&self.pool)
        .await
        .context("Failed to check for duplicate pool reading")?;
        
        if exists {
            info!("Pool reading from email {} already exists, skipping", email_id);
            return Ok(());
        }
        
        sqlx::query(
            r#"
            INSERT INTO pool_readings (timestamp, temperature, ph, orp, email_id)
            VALUES ($1, $2, $3, $4, $5)
            "#
        )
        .bind(reading.timestamp)
        .bind(reading.temperature)
        .bind(reading.ph)
        .bind(reading.orp)
        .bind(email_id)
        .execute(&self.pool)
        .await
        .context("Failed to insert pool reading")?;
        
        info!("✅ Pool reading saved: temp={:?}°C, pH={:?}, ORP={:?} mV", 
              reading.temperature, reading.ph, reading.orp);
        
        Ok(())
    }
    
    pub async fn close(self) -> Result<()> {
        info!("Closing database connection");
        self.pool.close().await;
        Ok(())
    }
}