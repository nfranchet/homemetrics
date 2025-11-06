# Gestion Automatique du Rafraîchissement des Tokens Gmail

## Problème

Les **tokens d'accès Gmail** (OAuth2 `access_token`) expirent après **1 heure**. 

En mode daemon, le programme peut tourner pendant des jours/semaines. Sans rafraîchissement automatique, le token expirerait et les requêtes Gmail échoueraient.

## Solution Implémentée

### Architecture

Le système utilise le **mécanisme automatique de yup-oauth2** avec un **appel API périodique** pour garantir que le token reste valide.

**Comment ça fonctionne** :

1. **Rafraîchissement automatique par yup-oauth2** :
   - La bibliothèque `yup-oauth2` détecte automatiquement les tokens expirés
   - Elle utilise le `refresh_token` pour obtenir un nouvel `access_token`
   - Sauvegarde automatiquement dans `gmail-token-cache.json` (via `persist_tokens_to_disk()`)

2. **Appel API périodique (toutes les 45 minutes)** :
   - Un gestionnaire de tâche en arrière-plan (`TokenRefreshManager`)
   - Fait un appel API léger (`get_profile()`) toutes les 45 minutes
   - Déclenche la vérification automatique de yup-oauth2
   - Si le token est proche de l'expiration, yup-oauth2 le rafraîchit automatiquement

### ⚠️ Important : Comportement du Cache

**Le fichier `gmail-token-cache.json` n'est PAS mis à jour à chaque appel `refresh_token()` !**

Il est mis à jour **uniquement quand un vrai refresh se produit** :
- ✅ Token obtenu lors de l'OAuth2 flow initial
- ✅ Token rafraîchi automatiquement par yup-oauth2 (quand proche de l'expiration)
- ❌ **PAS** lors d'un appel API si le token est encore valide (>5 min de vie)

**Ceci est normal et attendu !** Le cache ne change que lors d'un vrai refresh.

### Chronologie Typique

```
T=0min    : Démarrage daemon, token valide jusqu'à T=60min
            📁 Cache: expires_at = [2025,310,11,24,20,...]  (11:24 UTC)
            
T=45min   : 🔄 refresh_token() appelé (appel périodique)
            → get_profile() exécuté
            → yup-oauth2 vérifie : token valide encore 15min
            ✅ Appel API réussi
            ❌ PAS de refresh (token encore bon pour 15min)
            📁 Cache INCHANGÉ (normal !)
            
T=56min   : 📧 Traitement emails programmé
            → messages_list() exécuté
            → yup-oauth2 vérifie : token expire dans 4min
            🔄 Refresh automatique déclenché !
            ✅ Nouveau access_token obtenu
            📁 Cache MIS À JOUR: expires_at = [2025,310,12,56,...]  (12:56 UTC)
            ✅ Appels API réussis
            
T=101min  : 🔄 refresh_token() appelé (appel périodique)
            → get_profile() exécuté
            → yup-oauth2 vérifie : token valide encore 15min
            ✅ Appel API réussi
            ❌ PAS de refresh (token encore bon)
            📁 Cache INCHANGÉ (normal !)
            
T=112min  : 📧 Traitement emails programmé
            → yup-oauth2 vérifie : token expire dans 4min
            🔄 Refresh automatique déclenché !
            📁 Cache MIS À JOUR: expires_at = [2025,310,13,52,...]  (13:52 UTC)
```

**Conclusion** : Le cache est mis à jour environ **toutes les heures** (quand le vrai refresh se produit), pas toutes les 45 minutes.

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

**1. GmailClient avec refresh explicite** (`src/gmail_client.rs`) :
```rust
pub struct GmailClient {
    hub: Gmail<...>,
    auth: Arc<Mutex<Authenticator>>,  // ← Référence à l'authenticator pour refresh
}

pub async fn refresh_token(&self) -> Result<()> {
    // Force le refresh en appelant directement auth.token()
    // yup-oauth2 vérifie l'expiration et rafraîchit si nécessaire
    let auth = self.auth.lock().await;
    let scopes = &[Scope::Modify.as_ref()];
    
    auth.token(scopes).await?;  // ← Force la vérification et le refresh
    // Le token est automatiquement persisté dans gmail-token-cache.json
    Ok(())
}
```

**Différence clé** : Appeler `auth.token()` directement force yup-oauth2 à :
1. Vérifier si le token est expiré ou proche de l'expiration
2. Utiliser le `refresh_token` pour obtenir un nouveau `access_token` si nécessaire
3. **Persister le nouveau token dans le cache** automatiquement

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
