# HomeMetrics X-Sense Email Client

Ce projet Rust automatise la récupération et le traitement des données de température provenant des emails X-Sense.

## Fonctionnalités

- 📧 **Client IMAP** : Récupère automatiquement les emails de `support@x-sense.com`
- 📎 **Extraction de pièces jointes** : Parse les fichiers CSV, JSON, XML et texte
- 🌡️ **Traitement des données** : Extrait les mesures de température et d'humidité
- 🗄️ **Base de données TimescaleDB** : Stockage optimisé pour les séries temporelles
- 🔍 **Filtrage intelligent** : Ne traite que les emails avec titre "Votre exportation de"
- 📁 **Archivage automatique** : Déplace les emails traités vers `/homemetrics/xsense`
- 🔄 **Mode daemon** : Exécution planifiée automatique à des horaires configurables
- ⏰ **Scheduling flexible** : Définissez plusieurs horaires quotidiens de récupération

## Prérequis

- Rust 1.70+
- PostgreSQL avec extension TimescaleDB
- Accès IMAP à votre boîte mail
- Variables d'environnement configurées

## Installation

1. **Cloner et configurer le projet**
```bash
git clone <repository-url>
cd homemetrics
cp .env.example .env
```

2. **Configurer les variables d'environnement**
```bash
# Éditer le fichier .env avec vos paramètres
nano .env
```

3. **Installer et configurer TimescaleDB**
```bash
# Ubuntu/Debian
sudo apt install postgresql postgresql-contrib timescaledb-postgresql
sudo -u postgres createdb homemetrics
sudo -u postgres psql -d homemetrics -c "CREATE EXTENSION IF NOT EXISTS timescaledb CASCADE;"
```

4. **Compiler le projet**
```bash
cargo build --release
```

## Configuration

### Variables d'environnement requises

| Variable | Description | Exemple |
|----------|-------------|---------|
| `IMAP_SERVER` | Serveur IMAP | `imap.gmail.com` |
| `IMAP_PORT` | Port IMAP | `993` |
| `IMAP_USERNAME` | Nom d'utilisateur email | `user@gmail.com` |
| `IMAP_PASSWORD` | Mot de passe d'application | `app-password` |
| `DB_HOST` | Hôte PostgreSQL | `localhost` |
| `DB_PORT` | Port PostgreSQL | `5432` |
| `DB_NAME` | Nom de la base de données | `homemetrics` |
| `DB_USERNAME` | Utilisateur PostgreSQL | `postgres` |
| `DB_PASSWORD` | Mot de passe PostgreSQL | `password` |
| `SCHEDULER_ENABLED` | Activer le mode daemon | `true` ou `false` |
| `SCHEDULER_TIMES` | Horaires de récupération | `02:00,14:00` |
| `DATA_DIR` | Répertoire de sauvegarde | `./data` |

### Configuration Gmail

Pour Gmail, vous devez :
1. Activer l'authentification à 2 facteurs
2. Générer un mot de passe d'application
3. Utiliser ce mot de passe dans `IMAP_PASSWORD`

## Mode Dry-Run 🧪

Le mode dry-run permet de tester la connexion IMAP et d'analyser les emails **sans sauvegarde en base de données**.

### Fonctionnalités du Dry-Run

- ✅ **Connexion IMAP** : Teste la connexion au serveur mail
- ✅ **Recherche d'emails** : Trouve les emails X-Sense correspondants
- ✅ **Affichage du contenu** : Montre les headers et un aperçu du corps des emails
- ✅ **Extraction des pièces jointes** : Parse et sauvegarde les fichiers dans `./data/`
- ✅ **Préfixage par date** : Chaque fichier est préfixé par la date/heure
- ❌ **Pas de base de données** : Aucune connexion ni sauvegarde en base

### Options CLI

```bash
# Vérifier la configuration
cargo run -- --check-config

# Mode dry-run (analyse seulement)
cargo run -- --dry-run

# Limiter le nombre d'emails traités
cargo run -- --dry-run --limit 5

# Changer le répertoire de sauvegarde
cargo run -- --dry-run --data-dir ./exports

# Mode production (avec base de données)
cargo run
```

## Mode Daemon 🔄

Le mode daemon permet d'exécuter le programme en continu avec récupération planifiée des emails.

### Configuration du Scheduler

Ajoutez ces variables dans votre fichier `.env` :

```bash
# Activer le mode daemon
SCHEDULER_ENABLED=true

# Horaires de récupération (format HH:MM, séparés par des virgules)
# Exemple: récupération à 2h du matin et 14h
SCHEDULER_TIMES=02:00,14:00
```

### Utilisation

```bash
# Lancer en mode daemon
cargo run -- --daemon

# Mode daemon avec dry-run (analyse seulement, pas de sauvegarde DB)
cargo run -- --daemon --dry-run

# Mode daemon avec limite d'emails
cargo run -- --daemon --limit 10
```

### Fonctionnement

- ✅ Le programme tourne en continu
- ✅ Récupération automatique aux horaires configurés
- ✅ Chaque email traité est déplacé vers `/homemetrics/xsense`
- ✅ Les emails restent dans ce dossier et ne sont plus retraités
- ✅ Log périodique toutes les heures pour confirmer que le daemon est actif
- ✅ Arrêt propre avec Ctrl+C

### Archivage des Emails

Après traitement, les emails sont automatiquement :
1. Copiés vers le répertoire IMAP `/homemetrics/xsense`
2. Supprimés de la boîte de réception (INBOX)

Le répertoire `/homemetrics/xsense` est créé automatiquement s'il n'existe pas.

**Note** : En mode dry-run, les emails ne sont PAS déplacés (analyse seulement).

```

# Aide sur les options
cargo run -- --help
```

## Utilisation

### Test rapide (Mode Dry-Run)

```bash
# 1. Configurer les credentials IMAP
cp .env.example .env
# Éditer .env avec vos credentials IMAP

# 2. Test en mode dry-run (analyse seulement, sans base de données)
cargo run -- --dry-run --limit 3

# 3. Ou utiliser le script de vérification interactif
./test.sh
```

### Test complet avec base de données

```bash
# 1. Exécuter le script de vérification
./test.sh

# 2. Initialiser la base de données
psql -d homemetrics -f init_db.sql

# 3. Configurer les variables d'environnement
# Éditer .env et configurer DB_PASSWORD

# 4. Compiler et exécuter en mode production
cargo build --release
cargo run  # Sans --dry-run pour sauvegarder en base
```

### Exécution

```bash
# Test rapide (mode dry-run, 3 emails max)
cargo run -- --dry-run --limit 3

# Analyse complète sans base de données
cargo run -- --dry-run

# Mode production avec base de données
cargo run

# Mode production compilé
cargo build --release
./target/release/homemetrics --dry-run  # ou sans pour la production
```

### Logs

```bash
# Logs détaillés
RUST_LOG=debug cargo run

# Logs normaux
RUST_LOG=info cargo run
```

### Vérification des données

```bash
# Voir les dernières lectures
psql -d homemetrics -c "SELECT * FROM temperature_readings ORDER BY timestamp DESC LIMIT 10;"

# Statistiques par capteur
psql -d homemetrics -c "SELECT sensor_id, COUNT(*), AVG(temperature) as avg_temp FROM temperature_readings GROUP BY sensor_id;"
```

## Structure des données

### Formats supportés

Le système peut traiter les formats suivants dans les pièces jointes :

- **CSV** : Colonnes timestamp, sensor_id, temperature, [humidity], [location]
- **JSON** : Objets avec propriétés `timestamp`, `sensor_id`, `temperature`, etc.
- **XML** : Format X-Sense standard (en développement)
- **Texte** : Parsing avec regex pour extraire les données

### Base de données

```sql
-- Table des capteurs
CREATE TABLE sensors (
    id UUID PRIMARY KEY,
    sensor_id VARCHAR(255) UNIQUE NOT NULL,
    location VARCHAR(255),
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Table des lectures (hypertable TimescaleDB)
CREATE TABLE temperature_readings (
    id UUID PRIMARY KEY,
    sensor_id VARCHAR(255) REFERENCES sensors(sensor_id),
    timestamp TIMESTAMPTZ NOT NULL,
    temperature DOUBLE PRECISION NOT NULL,
    humidity DOUBLE PRECISION,
    location VARCHAR(255),
    processed_at TIMESTAMPTZ DEFAULT NOW()
);
```

## Déploiement en Production

### Installation avec Systemd

Le projet inclut un script de déploiement automatique pour installer le service en mode daemon :

```bash
# Exécuter le script de déploiement (en tant que root)
sudo ./deploy_daemon.sh
```

Ce script va :
1. Créer un utilisateur système `homemetrics`
2. Installer le binaire dans `/opt/homemetrics`
3. Créer le répertoire de données
4. Copier le fichier `.env.example` si nécessaire
5. Installer le service systemd

### Configuration du Service

Après l'installation, éditez la configuration :

```bash
# Éditer le fichier de configuration
sudo nano /opt/homemetrics/.env

# Assurez-vous d'activer le scheduler
SCHEDULER_ENABLED=true
SCHEDULER_TIMES=02:00,14:00
```

### Gestion du Service

```bash
# Démarrer le service
sudo systemctl start homemetrics

# Activer au démarrage
sudo systemctl enable homemetrics

# Voir le statut
sudo systemctl status homemetrics

# Voir les logs en temps réel
sudo journalctl -u homemetrics -f

# Redémarrer après modification de la configuration
sudo systemctl restart homemetrics

# Arrêter le service
sudo systemctl stop homemetrics
```

### Mise à jour

```bash
# Recompiler et redéployer
cargo build --release
sudo cp target/release/homemetrics /opt/homemetrics/
sudo systemctl restart homemetrics
```

## Développement

### Structure du projet

```
src/
├── main.rs              # Point d'entrée + mode daemon
├── config.rs            # Configuration + scheduler
├── imap_client.rs       # Client IMAP + archivage
├── attachment_parser.rs # Extraction pièces jointes
├── email_common.rs      # Structures communes
├── database.rs          # Interface TimescaleDB
├── xsense/
│   ├── mod.rs           # Module exports
│   ├── extractor.rs     # Parsing données température (CSV/JSON/XML)
│   └── processor.rs     # Orchestrateur X-Sense
└── blueriot/
    ├── mod.rs           # Module exports
    ├── extractor.rs     # Extraction métriques piscine (regex)
    └── processor.rs     # Orchestrateur Blue Riot
```

### Tests

```bash
cargo test
```

### Compilation optimisée

```bash
cargo build --release
```

## Dépendances principales

- `tokio` : Runtime async
- `imap` + `native-tls` : Client IMAP sécurisé
- `mail-parser` : Parsing des emails et pièces jointes
- `sqlx` : Interface PostgreSQL async
- `serde` + `csv` : Traitement des données
- `chrono` : Gestion des timestamps
- `anyhow` : Gestion d'erreurs

## Sécurité

- ✅ Connexions TLS pour IMAP et base de données
- ✅ Mots de passe via variables d'environnement
- ✅ Validation des données d'entrée
- ✅ Gestion des erreurs robuste
- ✅ Prévention des doublons en base

## Structure du projet

```
```
homemetrics/
├── src/
│   ├── main.rs              # Point d'entrée avec arguments CLI
│   ├── config.rs            # Configuration depuis variables d'env
│   ├── imap_client.rs       # Client IMAP sécurisé
│   ├── attachment_parser.rs # Extraction pièces jointes
│   ├── email_common.rs      # Structures communes
│   ├── database.rs          # Interface TimescaleDB
│   ├── xsense/
│   │   ├── mod.rs           # Module exports
│   │   ├── extractor.rs     # Parsing données température
│   │   └── processor.rs     # Orchestrateur X-Sense
│   └── blueriot/
│       ├── mod.rs           # Module exports
│       ├── extractor.rs     # Extraction métriques piscine
│       └── processor.rs     # Orchestrateur Blue Riot
├── .env.example             # Template de configuration
├── test.sh                  # Script de test interactif
```
├── test_env.sh              # Variables d'environnement de test
├── init_db.sql              # Initialisation base de données
└── data/                    # Répertoire pièces jointes (créé auto)
```

## Roadmap

- [ ] Interface web pour visualisation des données
- [ ] Support XML avancé pour formats X-Sense
- [ ] Notifications en cas d'anomalies
- [ ] API REST pour accès aux données
- [ ] Clustering multi-instances
- [ ] Sauvegarde automatique des données

## Licence

MIT License - voir le fichier LICENSE pour plus de détails.

## Support

Pour toute question ou problème, créez une issue sur GitHub ou consultez la documentation technique.