#!/bin/bash

# Script de test pour HomeMetrics X-Sense Email Client

echo "🚀 Test du client mail HomeMetrics X-Sense"
echo "=========================================="

# Vérifier si les variables d'environnement sont définies
check_env_var() {
    if [ -z "${!1}" ]; then
        echo "❌ Variable d'environnement $1 non définie"
        return 1
    else
        echo "✅ $1 définie"
        return 0
    fi
}

echo ""
echo "📋 Vérification des variables d'environnement..."

# Variables IMAP
check_env_var "IMAP_SERVER"
check_env_var "IMAP_USERNAME" 
check_env_var "IMAP_PASSWORD"

# Variables Base de données
check_env_var "DB_HOST"
check_env_var "DB_NAME"
check_env_var "DB_USERNAME"
check_env_var "DB_PASSWORD"

echo ""
echo "🔧 Configuration recommandée:"
echo "1. Copiez le fichier .env.example vers .env"
echo "2. Remplissez les variables d'environnement dans .env"
echo "3. Sourcez le fichier: source .env"
echo "4. Assurez-vous que TimescaleDB est démarré"

echo ""
echo "🐘 Pour installer TimescaleDB (Ubuntu/Debian):"
echo "sudo apt install postgresql postgresql-contrib timescaledb-postgresql"
echo "sudo -u postgres createdb homemetrics"
echo 'sudo -u postgres psql -d homemetrics -c "CREATE EXTENSION IF NOT EXISTS timescaledb CASCADE;"'

echo ""
echo "📧 Configuration Gmail:"
echo "1. Activez l'authentification à 2 facteurs"
echo "2. Générez un mot de passe d'application"
echo "3. Utilisez ce mot de passe dans IMAP_PASSWORD"

echo ""
echo "🎯 Test du projet:"
echo ""
echo "# Test rapide en mode dry-run (sans base de données)"
echo "cargo run -- --dry-run --limit 3"
echo ""
echo "# Compilation optimisée"
echo "cargo build --release"
echo ""
echo "# Mode production (avec base de données)"
echo "cargo run"

echo ""
echo "📊 Consultation des données:"
echo "psql -d homemetrics -c 'SELECT * FROM temperature_readings ORDER BY timestamp DESC LIMIT 10;'"

echo ""
echo "🧪 Test rapide maintenant:"
echo "Voulez-vous lancer un test dry-run ? (y/N)"
read -r response
if [[ "$response" =~ ^([yY][eE][sS]|[yY])$ ]]; then
    echo "🚀 Lancement du test dry-run..."
    cargo run -- --dry-run --limit 2
fi