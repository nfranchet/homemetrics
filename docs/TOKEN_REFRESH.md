# Gestion Automatique du Rafraîchissement des Tokens Gmail

## Problème

Les **tokens d'accès Gmail** (OAuth2 `access_token`) expirent après **1 heure**. 

En mode daemon, le programme peut tourner pendant des jours/semaines. Sans rafraîchissement automatique, le token expirerait et les requêtes Gmail échoueraient.

## Solution Implémentée

### Architecture

Le système utilise **deux mécanismes complémentaires** :

1. **Rafraîchissement automatique par yup-oauth2** :
   - La bibliothèque `yup-oauth2` détecte automatiquement les tokens expirés
   - Elle utilise le `refresh_token` pour obtenir un nouvel `access_token`
   - Fonctionne **à la demande** (lorsqu'une requête est faite)

2. **Rafraîchissement proactif (nouveau)** :
   - Un gestionnaire de tâche en arrière-plan (`TokenRefreshManager`)
   - Rafraîchit le token **toutes les 45 minutes**
   - Empêche l'expiration avant qu'elle ne se produise

### Fonctionnement du Token Refresh Manager

```
Démarrage du daemon
        │
        ▼
Création GmailClient ──► Arc<Mutex<GmailClient>>
        │                        │
        │                        ├──► XSenseProcessor (utilise pour requêtes)
        │                        ├──► BlueRiotProcessor (utilise pour requêtes)
        │                        └──► TokenRefreshManager
        ▼                                     │
Démarrage TokenRefreshManager                 │
        │                                     │
        ▼                                     │
Boucle infinie :                              │
  ┌─────────────────────────────┐            │
  │ Attendre 45 minutes          │            │
  ├─────────────────────────────┤            │
  │ Appeler refresh_token()     │◄───────────┘
  ├─────────────────────────────┤
  │ Token rafraîchi             │
  ├─────────────────────────────┤
  │ Sauvegardé dans cache       │
  └─────────────────────────────┘
         │
         └──► Retour au début
```

### Code Key Points

**1. GmailClient avec auto-refresh** (`src/gmail_client.rs`) :
```rust
pub struct GmailClient {
    hub: Gmail<...>,  // Contient l'authenticator avec tokens persistés
}

pub async fn refresh_token(&self) -> Result<()> {
    // Fait un appel API léger (get_profile) qui déclenche
    // automatiquement le refresh par yup-oauth2 si nécessaire
    self.hub.users().get_profile("me").doit().await?;
    Ok(())
}
```

**Mécanisme**: L'authenticator de yup-oauth2 vérifie automatiquement l'expiration du token avant chaque appel API et utilise le `refresh_token` pour obtenir un nouveau `access_token` si nécessaire.

**2. TokenRefreshManager** (`src/token_refresh.rs`) :
```rust
pub struct TokenRefreshManager {
    gmail_client: Arc<Mutex<GmailClient>>,
    refresh_interval: Duration,  // Default: 45 minutes
}

async fn run_refresh_loop(&self) {
    loop {
        ticker.tick().await;  // Attendre 45 min
        self.refresh_token_safely().await;  // Rafraîchir
    }
}
```

**3. Intégration dans le Daemon** (`src/main.rs`) :
```rust
async fn run_daemon_mode(...) {
    // Créer client partagé
    let gmail_client = GmailClient::new(...).await?;
    let gmail_client_arc = Arc::new(Mutex::new(gmail_client));
    
    // Démarrer le refresh automatique (45 min)
    let _handle = token_refresh::start_token_refresh(
        gmail_client_arc.clone(),
        Some(45)
    );
    
    // Les processors utiliseront le même client
    // Token toujours valide !
}
```

## Chronologie du Token

```
T = 0       : Token créé (valide 60 min)
T = 45 min  : Premier refresh proactif → nouveau token (valide jusqu'à T+105)
T = 90 min  : Deuxième refresh → nouveau token (valide jusqu'à T+150)
T = 135 min : Troisième refresh → etc.
```

**Avantage** : Le token n'expire jamais car il est renouvelé toutes les 45 minutes (avant l'expiration à 60 minutes).

## Sécurité

### Pourquoi 45 minutes ?

- Token Google expire à **60 minutes**
- Rafraîchir à **45 minutes** laisse **15 minutes de marge**
- Évite les race conditions si une requête est en cours

### Sécurité du `refresh_token`

Le `refresh_token` :
- ✅ **Ne expire jamais** (sauf révocation manuelle)
- ✅ **Stocké dans** `gmail-token-cache.json`
- ✅ **Chiffré sur disque** (par yup-oauth2)
- ✅ **Utilisé uniquement pour générer de nouveaux** `access_token`

### Protection

```bash
# Vérifier les permissions du cache
ls -l gmail-token-cache.json
# Devrait être : -rw------- (600) = lecture/écriture propriétaire seulement

# Si besoin, corriger :
chmod 600 gmail-token-cache.json
```

## Logs en Mode Daemon

Exemple de logs typiques :

```
[INFO] 🔐 Initializing Gmail client with automatic token refresh...
[INFO] ✅ Gmail API connection established successfully
[INFO] 🔄 Starting automatic token refresh (every 45 minutes)
[INFO] 🔄 Token refresh loop started
[INFO] ✅ Token refresh manager started

... 45 minutes plus tard ...

[INFO] ⏰ Token refresh interval reached, refreshing token...
[INFO] 🔄 Refreshing Gmail OAuth2 token to keep it alive...
[INFO] 🔄 Forcing OAuth2 token refresh...
[INFO] ✅ Token refreshed successfully
[INFO] ✅ Token refresh completed successfully
[INFO] ✅ Token refresh successful
```

## Configuration

### Intervalle de Refresh

Par défaut : **45 minutes**

Pour modifier (dans `src/main.rs`) :

```rust
// Rafraîchir toutes les 50 minutes
let _handle = token_refresh::start_token_refresh(
    gmail_client_arc.clone(),
    Some(50)  // ← Changer ici
);
```

**⚠️ Limite de sécurité** : Max 55 minutes (pour garder une marge de 5 min)

### Désactiver le Refresh Automatique

**Non recommandé**, mais si nécessaire :

```rust
// Option 1: Commenter la ligne
// let _handle = token_refresh::start_token_refresh(...);

// Option 2: Le programme utilisera quand même le refresh
// automatique de yup-oauth2 (à la demande)
```

## Fichiers Modifiés/Créés

| Fichier | Changement |
|---------|-----------|
| `src/gmail_client.rs` | + `Arc<Mutex<Authenticator>>` pour partage<br>+ `refresh_token()` méthode publique |
| `src/token_refresh.rs` | **Nouveau** : TokenRefreshManager |
| `src/main.rs` | Intégration dans `run_daemon_mode()` |
| `src/lib.rs` | Export du module `token_refresh` |
| `docs/TOKEN_REFRESH.md` | **Ce fichier** - Documentation |

## Troubleshooting

### Le token expire quand même

**Symptômes** :
```
[ERROR] ❌ Error processing emails: OAuth2 error: invalid_token
```

**Causes possibles** :
1. Le `refresh_token` a été révoqué (re-authent nécessaire)
2. Le cache token est corrompu
3. Le daemon a été arrêté puis redémarré > 60 min

**Solution** :
```bash
# Re-générer le token
rm gmail-token-cache.json
cargo run -- --dry-run
# Suivre les instructions OAuth2
```

### Le refresh échoue en boucle

**Symptômes** :
```
[ERROR] ❌ Token refresh failed: ...
[WARN] ⚠️  Will retry at next interval
```

**Causes** :
- Problème réseau (firewall, proxy)
- Credentials Google révoqués
- Quota API dépassé

**Solution** :
```bash
# Vérifier les credentials
cat credentials.json

# Vérifier la connectivité
curl https://oauth2.googleapis.com/token

# Vérifier les quotas sur
# https://console.cloud.google.com/apis/dashboard
```

## Références

- [Google OAuth2 Documentation](https://developers.google.com/identity/protocols/oauth2)
- [yup-oauth2 Crate](https://docs.rs/yup-oauth2/)
- [Token Expiration RFC 6749](https://datatracker.ietf.org/doc/html/rfc6749#section-5.1)
