-- Fix incorrect sensor_id and location values from X-Sense data
-- Problem: sensor_id contained full filename like "Bureau_Exporter les données_20251031"
-- Solution: Extract the sensor name from the beginning of the string

BEGIN;

-- Show current problematic data
SELECT 
    sensor_id, 
    location,
    COUNT(*) as count,
    MIN(timestamp) as first_reading,
    MAX(timestamp) as last_reading
FROM temperature_readings 
WHERE sensor_id LIKE '%Exporter%' OR sensor_id LIKE '%Export%'
GROUP BY sensor_id, location
ORDER BY sensor_id;

-- Option 1: DELETE incorrect data (RECOMMENDED)
-- This is safer as the filename-based sensor_id causes data quality issues

DELETE FROM temperature_readings 
WHERE sensor_id LIKE '%Exporter%' OR sensor_id LIKE '%Export%';

-- Show how many rows were affected
-- (PostgreSQL will show this automatically)

-- Option 2: UPDATE to fix sensor_id (ALTERNATIVE - more risky)
-- Uncomment if you prefer to salvage the data instead of deleting it
/*
UPDATE temperature_readings
SET 
    sensor_id = SPLIT_PART(sensor_id, '_', 1),
    location = SPLIT_PART(sensor_id, '_', 1)
WHERE sensor_id LIKE '%Exporter%' OR sensor_id LIKE '%Export%';
*/

-- Verify cleanup
SELECT 
    sensor_id, 
    location,
    COUNT(*) as count
FROM temperature_readings 
GROUP BY sensor_id, location
ORDER BY sensor_id;

-- Show the sensors table
SELECT sensor_id, location, created_at, updated_at
FROM sensors
ORDER BY created_at DESC;

COMMIT;

-- Optionally clean up orphaned sensors (sensors without any readings)
-- Uncomment to execute:
/*
BEGIN;
DELETE FROM sensors 
WHERE sensor_id NOT IN (
    SELECT DISTINCT sensor_id FROM temperature_readings
);
COMMIT;
*/
