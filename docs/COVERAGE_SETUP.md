# Configuration de la Couverture de Code avec Codecov

Ce document explique comment visualiser le pourcentage de couverture de code dans GitHub.

## Solutions Disponibles

### 1. Codecov.io (Recommandé) ✅

**Avantages :**
- Badge dynamique avec pourcentage exact dans le README
- Graphiques de tendance de couverture
- Commentaires automatiques sur les Pull Requests
- Gratuit pour projets open source
- Interface web détaillée pour explorer la couverture

**Setup :**

1. **Créer un compte Codecov :**
   - Aller sur https://codecov.io
   - Se connecter avec GitHub
   - Autoriser l'accès au repository `nfranchet/homemetrics`

2. **Obtenir le token Codecov :**
   - Sur Codecov.io, aller dans Settings du repository
   - Copier le `CODECOV_TOKEN`

3. **Ajouter le token dans GitHub :**
   - Aller dans `Settings` > `Secrets and variables` > `Actions`
   - Cliquer sur `New repository secret`
   - Nom : `CODECOV_TOKEN`
   - Valeur : Coller le token de Codecov
   - Cliquer sur `Add secret`

4. **Push et vérifier :**
   ```bash
   git add .
   git commit -m "Add Codecov integration"
   git push
   ```
   
5. **Résultat :**
   - Le badge dans le README affichera le pourcentage exact : ![14.08%](https://img.shields.io/badge/coverage-14.08%25-red)
   - Chaque commit mettra à jour le pourcentage
   - Les PRs auront un commentaire avec les changements de couverture

### 2. GitHub Actions Summary (Déjà configuré) ✅

Le workflow `coverage.yml` génère déjà un rapport que vous pouvez voir :
- Aller dans l'onglet `Actions`
- Cliquer sur le workflow `test-coverage`
- Télécharger l'artifact `coverage-report`
- Ouvrir `coverage/tarpaulin-report.html` dans un navigateur

**Voir les logs :**
Dans les logs du workflow, vous verrez :
```
14.08% coverage, 164/1165 lines covered
```

### 3. Alternative : Coveralls.io

Si vous préférez Coveralls à Codecov :

1. Aller sur https://coveralls.io
2. Connecter avec GitHub
3. Activer le repository
4. Modifier `.github/workflows/coverage.yml` :

```yaml
- name: Upload to Coveralls
  uses: coverallsapp/github-action@v2
  with:
    github-token: ${{ secrets.GITHUB_TOKEN }}
    path-to-lcov: ./coverage/lcov.info
```

5. Badge pour README :
```markdown
[![Coverage Status](https://coveralls.io/repos/github/nfranchet/homemetrics/badge.svg?branch=main)](https://coveralls.io/github/nfranchet/homemetrics?branch=main)
```

### 4. Alternative : Générer un badge local

Sans service externe, vous pouvez générer un badge statique :

```bash
# Après avoir exécuté cargo tarpaulin
COVERAGE=$(grep -oP '\d+\.\d+(?=% coverage)' coverage_output.txt)
echo "[![Coverage](https://img.shields.io/badge/coverage-${COVERAGE}%25-brightgreen)]"
```

## État Actuel

✅ **Déjà configuré :**
- Workflow GitHub Actions pour la couverture
- Badge Codecov dans le README
- Génération de rapports XML et HTML
- Upload d'artifacts

⏳ **À faire :**
- Créer un compte sur Codecov.io
- Ajouter le `CODECOV_TOKEN` dans les secrets GitHub
- Push du code pour activer l'intégration

## Voir la Couverture Localement

```bash
# Générer le rapport de couverture
./scripts/coverage.sh

# Ouvrir le rapport HTML
open coverage/tarpaulin-report.html  # macOS
xdg-open coverage/tarpaulin-report.html  # Linux
```

## Améliorer la Couverture

Pour augmenter le pourcentage de couverture (actuellement 14.08%) :

1. **Ajouter des tests d'intégration** pour les modules non couverts :
   - `src/gmail_client.rs` (0%)
   - `src/database.rs` (0%)
   - `src/xsense/processor.rs` (0%)
   - `src/blueriot/processor.rs` (0%)

2. **Exécuter les tests de base de données** (actuellement ignorés) :
   ```bash
   ./scripts/setup_test_db.sh
   cargo test --test database_test -- --ignored
   ```

3. **Augmenter la couverture des extractors** :
   - `src/xsense/extractor.rs` : 32% → objectif 80%
   - `src/attachment_parser.rs` : 36% → objectif 70%

## Objectifs de Couverture

- **Court terme** : 30% (tests extractors complets)
- **Moyen terme** : 50% (tests database + processors)
- **Long terme** : 70% (tests Gmail client + intégration complète)

## Liens Utiles

- [Codecov Documentation](https://docs.codecov.com/docs)
- [GitHub Actions Secrets](https://docs.github.com/en/actions/security-guides/encrypted-secrets)
- [cargo-tarpaulin Guide](https://github.com/xd009642/tarpaulin)
