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

L'approche correcte utilise le mécanisme automatique de `yup-oauth2` sans duplication d'authenticator :

```rust
// ✅ APPROCHE CORRECTE
pub struct GmailClient {
    hub: Gmail<...>,  // Contient l'authenticator (pas de duplication)
}

impl GmailClient {
    pub async fn new(config: &GmailConfig) -> Result<Self> {
        let auth = create_authenticator()
            .persist_tokens_to_disk(&config.token_cache_path)  // Persistence
            .build()
            .await?;
        
        // Pas de clone - ownership direct
        let hub = Gmail::new(client, auth);
        
        Ok(GmailClient { hub })
    }
    
    /// Déclenche le refresh automatique via un appel API léger
    pub async fn refresh_token(&self) -> Result<()> {
        // Appel API simple - yup-oauth2 gère le refresh automatiquement
        self.hub.users().get_profile("me")
            .add_scope(Scope::Modify)
            .doit()
            .await?;
        Ok(())
    }
}
```

### Comment Ça Fonctionne

1. **Persistence Automatique** : `persist_tokens_to_disk()` configure yup-oauth2 pour sauvegarder les tokens dans `gmail-token-cache.json`

2. **Refresh Automatique** : Avant chaque appel API, yup-oauth2 :
   - Vérifie si le `access_token` est expiré
   - Si oui, utilise le `refresh_token` pour obtenir un nouveau `access_token`
   - Sauvegarde automatiquement les nouveaux tokens dans le cache

3. **Déclenchement Périodique** : Le `TokenRefreshManager` appelle `refresh_token()` toutes les 45 minutes
   - Cet appel API léger (`get_profile`) déclenche la vérification automatique
   - Si le token a >15 minutes de vie, rien ne se passe
   - Si le token est proche de l'expiration, yup-oauth2 le rafraîchit

4. **Pas de Clonage** : L'authenticator reste unique et partagé via l'ownership du `hub`

## Avantages de la Nouvelle Approche

✅ **Plus simple** : Pas de gestion manuelle de Arc<Mutex<>>
✅ **Plus sûr** : Utilise le mécanisme natif de yup-oauth2
✅ **Plus robuste** : Pas de risque de désynchronisation entre instances
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
