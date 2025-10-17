#!/bin/bash
set -e

psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<-EOSQL
	CREATE USER homemetric PASSWORD 'homemetric' createdb;
	CREATE DATABASE homemetric OWNER homemetric;
	ALTER DATABASE homemetric SET log_statement = 'all';
	GRANT ALL ON SCHEMA public TO homemetric;
	GRANT ALL PRIVILEGES ON DATABASE homemetric TO homemetric;
EOSQL
