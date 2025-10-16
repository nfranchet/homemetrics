# Variables d'environnement de test - NE PAS UTILISER EN PRODUCTION

# Configuration IMAP de test (utiliser vos vraies credentials)
export IMAP_SERVER="imap.gmail.com"
export IMAP_PORT="993"
export IMAP_USERNAME="your-email@gmail.com"
export IMAP_PASSWORD="your-app-password"
export IMAP_MAILBOX="INBOX"

# Configuration Base de Données de test
export DB_HOST="localhost"
export DB_PORT="5432"
export DB_NAME="homemetrics"
export DB_USERNAME="postgres"
export DB_PASSWORD="test-password"

# Configuration Logging
export RUST_LOG="info"

echo "Variables d'environnement de test chargées"
echo "⚠️  Remplacez par vos vraies credentials avant utilisation !"