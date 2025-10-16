-- Script d'initialisation de la base de données HomeMetrics
-- À exécuter avec: psql -d homemetrics -f init_db.sql

-- Créer l'extension TimescaleDB
CREATE EXTENSION IF NOT EXISTS timescaledb CASCADE;

-- Créer la table des capteurs
CREATE TABLE IF NOT EXISTS sensors (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sensor_id VARCHAR(255) UNIQUE NOT NULL,
    location VARCHAR(255),
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Créer la table des lectures de température
CREATE TABLE IF NOT EXISTS temperature_readings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sensor_id VARCHAR(255) NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    temperature DOUBLE PRECISION NOT NULL,
    humidity DOUBLE PRECISION,
    location VARCHAR(255),
    processed_at TIMESTAMPTZ DEFAULT NOW(),
    FOREIGN KEY (sensor_id) REFERENCES sensors(sensor_id) ON DELETE CASCADE
);

-- Créer une hypertable TimescaleDB pour les lectures de température
SELECT create_hypertable('temperature_readings', 'timestamp', if_not_exists => TRUE);

-- Créer des index pour optimiser les requêtes
CREATE INDEX IF NOT EXISTS idx_temp_readings_sensor_time 
ON temperature_readings (sensor_id, timestamp DESC);

CREATE INDEX IF NOT EXISTS idx_temp_readings_timestamp 
ON temperature_readings (timestamp DESC);

-- Insérer quelques capteurs de test
INSERT INTO sensors (sensor_id, location) VALUES 
    ('SENSOR001', 'Living Room'),
    ('SENSOR002', 'Bedroom'),
    ('SENSOR003', 'Kitchen')
ON CONFLICT (sensor_id) DO NOTHING;

-- Afficher un résumé
\echo 'Base de données HomeMetrics initialisée avec succès!'
\echo 'Tables créées:'
\d sensors
\d temperature_readings

\echo 'Capteurs de test:'
SELECT * FROM sensors;