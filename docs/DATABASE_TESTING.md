# Database Testing Guide

This document explains how to run and write database integration tests for the homemetrics project.

## Overview

The database tests are **integration tests** that verify the actual PostgreSQL database interactions. They are marked with `#[ignore]` by default to avoid requiring a database for normal test runs.

## Test Structure

- **Location**: `tests/database_test.rs`
- **Type**: Integration tests (require PostgreSQL)
- **Coverage**: 
  - Database connection
  - Temperature readings storage
  - Pool readings storage
  - Duplicate detection
  - Multi-sensor handling
  - Partial data handling (NULL values)

## Running Database Tests

### Prerequisites

1. PostgreSQL server running (local or remote)
2. PostgreSQL client tools installed (`psql`)

### Setup Test Database

Run the automated setup script:

```bash
./scripts/setup_test_db.sh
```

This script will:
- Create a test database (`homemetrics_test` by default)
- Enable TimescaleDB extension (if available)
- Configure environment variables

### Run Tests

```bash
# Run all database tests
cargo test --test database_test -- --ignored

# Run a specific test
cargo test --test database_test test_save_temperature_readings -- --ignored

# Run with output
cargo test --test database_test -- --ignored --nocapture
```

### Custom Database Configuration

Set environment variables to use a different database:

```bash
export TEST_DB_HOST=localhost
export TEST_DB_PORT=5432
export TEST_DB_NAME=my_test_db
export TEST_DB_USERNAME=postgres
export TEST_DB_PASSWORD=mypassword

cargo test --test database_test -- --ignored
```

Or inline:

```bash
TEST_DB_NAME=homemetrics_test TEST_DB_USERNAME=postgres TEST_DB_PASSWORD=postgres \
  cargo test --test database_test -- --ignored
```

## Test Coverage

Current database tests:

| Test | Description |
|------|-------------|
| `test_database_connection` | Verifies database connection works |
| `test_save_temperature_readings` | Tests saving temperature data |
| `test_save_pool_reading` | Tests saving pool metrics |
| `test_save_empty_readings` | Tests empty input handling |
| `test_multiple_sensors` | Tests multi-sensor data |
| `test_pool_reading_with_partial_data` | Tests NULL value handling |

## Why Integration Tests?

We use **integration tests** instead of mocks for database code because:

1. **Real behavior**: Tests actual PostgreSQL features (transactions, constraints, indexes)
2. **TimescaleDB**: Tests hypertable creation and time-series optimization
3. **SQL correctness**: Validates actual SQL queries work
4. **Foreign keys**: Tests referential integrity
5. **No mocking complexity**: Avoids complex mock setup for database connections

## CI/CD Considerations

In GitHub Actions or CI environments:

1. Set up a PostgreSQL service container
2. Run `scripts/setup_test_db.sh`
3. Execute tests with `cargo test --test database_test -- --ignored`

Example GitHub Actions workflow:

```yaml
services:
  postgres:
    image: timescale/timescaledb:latest-pg15
    env:
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: postgres
      POSTGRES_DB: homemetrics_test
    ports:
      - 5432:5432

steps:
  - name: Run database tests
    env:
      TEST_DB_NAME: homemetrics_test
      TEST_DB_USERNAME: postgres
      TEST_DB_PASSWORD: postgres
    run: cargo test --test database_test -- --ignored
```

## Writing New Database Tests

Template for new tests:

```rust
#[tokio::test]
#[ignore] // Always mark database tests with #[ignore]
async fn test_my_feature() {
    let config = get_test_db_config();
    let db = Database::new(&config)
        .await
        .expect("Failed to connect to test database");
    
    // Your test logic here
    
    // Use expect() for clearer error messages
    // Use assertions to verify behavior
    
    println!("✅ Test passed");
}
```

## Cleanup

The test database can be dropped after testing:

```bash
psql -U postgres -c "DROP DATABASE homemetrics_test;"
```

Or re-run the setup script to start fresh:

```bash
./scripts/setup_test_db.sh
```
