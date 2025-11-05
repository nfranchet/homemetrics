#!/bin/bash
# Script pour générer la couverture de tests détaillée
# Utilise cargo-tarpaulin pour analyser la couverture du code

set -e

echo "🧪 Génération de la couverture de tests..."
echo ""

# Nettoie les anciens rapports
rm -rf target/coverage
mkdir -p target/coverage

# Exécute tarpaulin avec la configuration
cargo tarpaulin \
    --config tarpaulin.toml \
    --engine llvm \
    --follow-exec \
    --post-test-delay 1 \
    --release

echo ""
echo "✅ Couverture générée avec succès!"
echo ""
echo "📊 Rapports disponibles:"
echo "   - HTML détaillé: target/coverage/index.html"
echo "   - LCOV:          target/coverage/lcov.info"
echo "   - JSON:          target/coverage/tarpaulin-report.json"
echo ""
echo "🌐 Ouvrir le rapport HTML:"
echo '   $BROWSER target/coverage/index.html'
echo ""
