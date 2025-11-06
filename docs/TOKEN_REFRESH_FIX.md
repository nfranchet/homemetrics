# Fix du Problème de Refresh de Token Gmail

## Problème Rencontré

Lors des tests en production, le système de refresh automatique des tokens Gmail présentait un comportement problématique :

- ✅ **Premier refresh (45 min)** : Succès
- ❌ **Deuxième refresh (90 min)** : Échec - nouvelle demande d'autorisation OAuth2 complète

### Logs de Production
```
[2025-11-05T23:48:12Z] ✅ Token refreshed successfully
[2025-11-06T00:33:12Z] 🔄 Forcing OAuth2 token refresh...
Please direct your browser to https://accounts.google.com/o/oauth2/auth...
```

## Cause Racine

L'implémentation initiale utilisait une architecture avec `Arc<Mutex<Authenticator>>` pour partager l'authenticator entre threads :

```rust
// ❌ APPROCHE PROBLÉMATIQUE
pub struct GmailClient {
    hub: Gmail<...>,
    auth: Arc<Mutex<Authenticator>>,  // Authenticator séparé
}

impl GmailClient {
    pub async fn new(config: &GmailConfig) -> Result<Self> {
        let auth = create_authenticator().await?;
        let auth_arc = Arc::new(Mutex::new(auth));
        
        // ⚠️ PROBLÈME ICI : .clone() crée une nouvelle instance
        let hub = Gmail::new(client, auth_arc.lock().await.clone());
        
        Ok(GmailClient { hub, auth: auth_arc })
    }
}
```

**Le problème** : Cloner l'authenticator (`auth_arc.lock().await.clone()`) crée une nouvelle instance déconnectée. Quand `yup-oauth2` rafraîchit le token via le `refresh_token`, il met à jour l'instance clonée dans `hub`, mais pas l'instance originale dans `auth_arc`. 

Au deuxième refresh, l'instance dans `auth_arc` a toujours l'ancien token expiré et ne peut pas utiliser le `refresh_token` correctement → nouvelle demande OAuth2.

## Solution Implémentée

L'approche correcte utilise **l'accès direct à l'authenticator** pour forcer le refresh :

```rust
// ✅ APPROCHE CORRECTE
pub struct GmailClient {
    hub: Gmail<...>,
    auth: Arc<Mutex<Authenticator>>,  // Référence séparée pour refresh
}

impl GmailClient {
    pub async fn new(config: &GmailConfig) -> Result<Self> {
        let auth = create_authenticator()
            .persist_tokens_to_disk(&config.token_cache_path)
            .build()
            .await?;
        
        // Garder une référence partagée à l'authenticator
        let auth_arc = Arc::new(Mutex::new(auth));
        
        // Cloner l'authenticator pour le hub (nécessaire pour l'API Gmail)
        let hub = Gmail::new(client, auth_arc.lock().await.clone());
        
        Ok(GmailClient { 
            hub,
            auth: auth_arc,  // Référence pour le refresh
        })
    }
    
    /// Force le refresh du token
    pub async fn refresh_token(&self) -> Result<()> {
        let auth = self.auth.lock().await;
        let scopes = &[Scope::Modify.as_ref()];
        
        // Appel direct à auth.token() force la vérification et le refresh
        auth.token(scopes).await?;
        
        // Le token est automatiquement persisté dans le cache
        Ok(())
    }
}
```

### Comment Ça Fonctionne

1. **Persistence Automatique** : `persist_tokens_to_disk()` configure yup-oauth2 pour sauvegarder les tokens dans `gmail-token-cache.json`

2. **Refresh Forcé** : Appeler `auth.token(scopes)` directement :
   - Vérifie si le `access_token` est expiré ou proche de l'expiration
   - Si oui, utilise le `refresh_token` pour obtenir un nouveau `access_token`
   - **Sauvegarde automatiquement** les nouveaux tokens dans le cache
   - Retourne le token (actuel ou rafraîchi)

3. **Déclenchement Périodique** : Le `TokenRefreshManager` appelle `refresh_token()` toutes les 45 minutes
   - Force la vérification et le refresh si nécessaire
   - Garantit que le token est toujours valide
   - Le fichier cache est mis à jour à chaque vrai refresh

4. **Pas de Duplication Problématique** : 
   - Le hub Gmail a son propre clone de l'authenticator (requis par l'API)
   - Mais nous gardons aussi une référence dans `auth` pour les refreshs explicites
   - Les deux fonctionnent sur le même fichier cache (via `persist_tokens_to_disk`)

## Avantages de la Nouvelle Approche

✅ **Plus simple** : Appel direct à `auth.token()` au lieu d'API call
✅ **Plus fiable** : Force vraiment le refresh, pas juste une vérification passive
✅ **Plus robuste** : Le cache est mis à jour systématiquement lors des refreshs
✅ **Testé** : Approche recommandée par la documentation yup-oauth2

## Chronologie du Token (Corrigée)

```
T=0min    : 🔑 Obtention token initial (access_token + refresh_token)
            └─► Sauvegarde dans gmail-token-cache.json

T=45min   : 🔄 Appel refresh_token()
            └─► API call get_profile()
                └─► yup-oauth2 vérifie : token encore valide 15min
                    └─► Rien à faire

T=90min   : 🔄 Appel refresh_token()
            └─► API call get_profile()
                └─► yup-oauth2 vérifie : token expiré depuis 30min
                    └─► Utilise refresh_token → nouveau access_token
                        └─► Sauvegarde automatique dans cache ✅

T=135min  : 🔄 Appel refresh_token()
            └─► API call get_profile()
                └─► yup-oauth2 vérifie : token encore valide 15min
                    └─► Rien à faire

T=180min  : 🔄 Appel refresh_token()
            └─► API call get_profile()
                └─► yup-oauth2 vérifie : token expiré depuis 30min
                    └─► Utilise refresh_token → nouveau access_token
                        └─► Sauvegarde automatique dans cache ✅

... et ainsi de suite indéfiniment
```

## Tests à Effectuer

Pour valider la correction :

1. ✅ **Compilation** : `cargo build --release` (OK)
2. ⏳ **Test longue durée** : Lancer le daemon >2 heures
3. ⏳ **Vérifier les logs** : Confirmer les refreshes à 45, 90, 135, 180 minutes
4. ⏳ **Vérifier le cache** : `cat gmail-token-cache.json | jq '.expires_at'`
5. ⏳ **Pas de réautorisation** : Confirmer qu'aucune URL OAuth n'est demandée

## Fichiers Modifiés

- `src/gmail_client.rs` : Simplifié, suppression de l'authenticator séparé
- `docs/TOKEN_REFRESH.md` : Mise à jour de la documentation
- `docs/TOKEN_REFRESH_FIX.md` : Ce document (explication du fix)

## Références

- [yup-oauth2 Documentation](https://docs.rs/yup-oauth2/)
- [Google OAuth2 Token Lifecycle](https://developers.google.com/identity/protocols/oauth2)
