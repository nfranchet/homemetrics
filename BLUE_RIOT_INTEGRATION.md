# Blue Riot Pool Monitoring Integration

## Overview

HomeMetrics now processes **two types of emails in parallel**:

1. **X-Sense Temperature Sensors** (`homemetrics/todo/xsense`)
   - Temperature and humidity data from CSV attachments
   - Stored in `temperature_readings` table

2. **Blue Riot Pool Monitoring** (`homemetrics/todo/blueriot`)  
   - Pool water quality metrics from email text
   - Stored in `pool_readings` table

## Pool Metrics Collected

| Metric | Description | Unit | Optimal Range |
|--------|-------------|------|---------------|
| **Temperature** | Pool water temperature | °C | 26-28°C |
| **pH** | Acidity/alkalinity level | - | 7.0-7.6 |
| **ORP** | Oxidation-Reduction Potential | mV | 650-750 mV |

## Database Schema

```sql
CREATE TABLE pool_readings (
    id SERIAL,
    timestamp TIMESTAMPTZ NOT NULL,
    temperature NUMERIC(5,2),
    ph NUMERIC(4,2),
    orp INTEGER,
    email_id VARCHAR(255),
    created_at TIMESTAMPTZ DEFAULT NOW(),
    PRIMARY KEY (id, timestamp)
);
```

## Email Processing Flow

### Blue Riot Emails

1. **Search** for emails with label `homemetrics/todo/blueriot`
2. **Parse** email text content (text/plain or text/html)
3. **Extract** metrics using regex patterns:
   - Temperature: `Température : 15.8 °C`
   - pH: `pH : 6.8`
   - ORP: `ORP : 249 mV`
4. **Save** to `pool_readings` table
5. **Mark as processed**:
   - Mark as **read**
   - Remove from **INBOX**
   - Change label: `homemetrics/todo/blueriot` → `homemetrics/done/blueriot`
6. **Notify** via Slack (if configured)

## Setup

### 1. Database Initialization

```bash
psql -d homemetric -f init_pool_db.sql
```

This creates:
- `pool_readings` table
- TimescaleDB hypertable for time-series optimization
- Indexes for efficient queries

### 2. Gmail Labels

Create the following labels in Gmail:
- `homemetrics/todo/blueriot` - Unprocessed pool emails
- `homemetrics/done/blueriot` - Processed pool emails

### 3. Email Filters

Create a Gmail filter to automatically label Blue Riot emails:
- **From**: `blue-connect@fluidra.com` (or your pool system sender)
- **Subject**: Contains "nouvelle mesure" or similar
- **Action**: Apply label `homemetrics/todo/blueriot`

## Usage

### Dry-Run Mode (Testing)

```bash
# Process both X-Sense and Blue Riot emails (no database)
cargo run -- --dry-run --limit 5

# Output example:
# 🧪 MODE DRY-RUN - POOL METRICS ANALYSIS (BLUE RIOT)
# 📧 Email: Piscine Pibrac : nouvelle mesure
# 📅 Date: 2025-11-04 11:18:15 UTC
# 🏊 Pool Metrics Extracted:
#    🌡️  Temperature: 15.8°C
#    🧪 pH: 6.80
#    ⚡ ORP: 249 mV
```

### Production Mode

```bash
# Process and save to database
cargo run -- --limit 10

# Both X-Sense and Blue Riot emails are processed in parallel
```

### Daemon Mode

```bash
# Schedule automatic processing
SCHEDULER_ENABLED=true SCHEDULER_TIMES="02:00,14:00" cargo run -- --daemon
```

## Parallel Processing

The system uses **tokio::join!** to process both email types concurrently:

```rust
let (xsense_result, pool_result) = tokio::join!(
    xsense_processor.process_emails(limit),
    pool_processor.process_emails(limit)
);
```

This means:
- ✅ Faster processing (both types run simultaneously)
- ✅ Independent error handling (one failure doesn't block the other)
- ✅ Shared resources (same Gmail client, database pool)

## Querying Pool Data

### Latest Readings

```sql
SELECT 
    timestamp,
    temperature,
    ph,
    orp
FROM pool_readings
ORDER BY timestamp DESC
LIMIT 10;
```

### Daily Averages

```sql
SELECT 
    DATE(timestamp) as date,
    AVG(temperature) as avg_temp,
    AVG(ph) as avg_ph,
    AVG(orp) as avg_orp
FROM pool_readings
WHERE timestamp > NOW() - INTERVAL '30 days'
GROUP BY DATE(timestamp)
ORDER BY date DESC;
```

### Out-of-Range Alerts

```sql
SELECT 
    timestamp,
    temperature,
    ph,
    orp,
    CASE 
        WHEN ph < 7.0 THEN 'pH too low'
        WHEN ph > 7.6 THEN 'pH too high'
        WHEN orp < 650 THEN 'ORP too low'
        WHEN orp > 750 THEN 'ORP too high'
        ELSE 'OK'
    END as status
FROM pool_readings
WHERE timestamp > NOW() - INTERVAL '7 days'
    AND (ph < 7.0 OR ph > 7.6 OR orp < 650 OR orp > 750)
ORDER BY timestamp DESC;
```

## Text Extraction Patterns

The `pool_extractor` module uses regex to extract metrics from email text:

### Temperature Patterns
- `Température : 15.8 °C`
- `Temperature: 25.5°C`
- `Temp: 26.2`

### pH Patterns
- `pH : 7.2`
- `pH: 7,4` (handles comma decimal separator)
- `ph = 7.15`

### ORP Patterns
- `ORP : 720 mV`
- `Redox: 680`
- `ORP: 750mV`

## Error Handling

- ❌ **No metrics found**: Email skipped, error logged
- ❌ **Parse error**: Email skipped, Slack notification sent
- ❌ **Database error**: Transaction rolled back, email not marked as processed
- ✅ **Partial data**: Metrics saved with NULL for missing values

## Slack Notifications

When configured, Slack notifications include:

```
🏊 New pool reading: 🌡️ 15.8°C | 🧪 pH 6.80 | ⚡ 249 mV
From: Piscine Pibrac : nouvelle mesure
```

## Testing

```bash
# Run tests for pool extractor
cargo test --lib pool_extractor

# Test specific patterns
cargo test test_extract_temperature
cargo test test_extract_ph
cargo test test_extract_orp
```

## Troubleshooting

### No metrics extracted

Check the email text format:
```bash
cargo run -- --dry-run --limit 1
# Look at "Text content" output
```

### Wrong timestamp

The timestamp is taken from the **email date**, not the measurement time. If your pool system includes a timestamp in the email, you can modify `pool_extractor.rs` to parse it.

### Database connection errors

```bash
# Check TimescaleDB is running
psql -d homemetric -c "SELECT version();"

# Verify pool_readings table exists
psql -d homemetric -c "\d pool_readings"
```

## Architecture

```
main.rs
├─ EmailProcessor (X-Sense)
│  ├─ gmail_client.search_xsense_emails()
│  ├─ attachment_parser
│  ├─ temperature_extractor
│  └─ database.save_temperature_reading()
│
└─ PoolEmailProcessor (Blue Riot)
   ├─ gmail_client.search_pool_emails()
   ├─ mail_parser (extract text from email)
   ├─ pool_extractor (regex extraction)
   └─ database.save_pool_reading()
```

Both processors run **in parallel** using `tokio::join!`.

## Future Enhancements

- [ ] Support more pool monitoring systems (Hayward, Pentair, etc.)
- [ ] Alert system for out-of-range values
- [ ] Grafana dashboard for visualization
- [ ] Mobile app notifications
- [ ] Historical trend analysis
- [ ] Predictive maintenance alerts
