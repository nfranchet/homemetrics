# Migration vers Gmail API REST avec OAuth2

## Vue d'ensemble

Le client HomeMetrics utilise l'API REST Gmail avec OAuth2 pour une authentification directe et sécurisée.

## Configuration requise

### 1. Créer un projet Google Cloud et activer l'API Gmail

1. Allez sur [Google Cloud Console](https://console.cloud.google.com/)
2. Créez un nouveau projet ou sélectionnez-en un existant
3. Activez l'API Gmail :
   - Navigation menu → APIs & Services → Library
   - Recherchez "Gmail API" et cliquez sur Enable

### 2. Créer des credentials OAuth2

1. Allez dans APIs & Services → Credentials
2. Cliquez sur "Create Credentials" → "OAuth client ID"
3. Si demandé, configurez l'écran de consentement OAuth :
   - Choisissez "External" si vous n'êtes pas dans une organisation Google Workspace
   - Remplissez les informations requises (nom de l'app, email de support, etc.)
   - Dans la section "Scopes", cliquez sur "Add or Remove Scopes"
   - Ajoutez les scopes suivants **(IMPORTANT - tous ces scopes sont requis)** :
     - `.../auth/gmail.readonly` - Lire tous les emails
     - `.../auth/gmail.modify` - Modifier les emails (pour ajouter/supprimer des labels)
     - `.../auth/gmail.labels` - Gérer les labels
   - Note: Vous pouvez aussi utiliser le scope complet `.../auth/gmail.metadata` ou `.../auth/gmail.readonly`
   - Sauvegardez et continuez

4. Revenez à "Create Credentials" → "OAuth client ID"
5. Choisissez "Desktop app" comme type d'application
6. Donnez-lui un nom (ex: "HomeMetrics Desktop Client")
7. Cliquez sur "Create"
8. Téléchargez le fichier JSON (bouton "Download JSON")

### 3. Configuration du projet

1. Copiez le fichier de credentials téléchargé dans votre projet :
   ```bash
   cp ~/Downloads/client_secret_*.json ./gmail-credentials.json
   chmod 600 ./gmail-credentials.json
   ```

2. Créez votre fichier `.env` :
   ```bash
   cp .env.example .env
   ```

3. Éditez `.env` avec vos valeurs :
   ```bash
   GMAIL_CREDENTIALS_PATH=./gmail-credentials.json
   GMAIL_TOKEN_CACHE_PATH=./gmail-token-cache.json
   # ... autres variables
   ```

### 4. Première authentification

Lors du premier lancement, le programme :
1. Ouvrira automatiquement votre navigateur
2. Vous demandera de vous connecter avec votre compte Gmail
3. Vous demandera d'autoriser l'application
4. Sauvegardera le token dans `gmail-token-cache.json`

Les lancements suivants utiliseront automatiquement le token sauvegardé.

## Avantages de l'OAuth2 Client

- ✅ **Plus simple** : Pas besoin de Service Account ou Domain-Wide Delegation
- ✅ **Authentification directe** : Connexion avec votre propre compte Gmail
- ✅ **Token persistant** : Pas besoin de se reconnecter à chaque fois
- ✅ **Plus sécurisé** : OAuth2 avec refresh token automatique

## Configuration

Variables d'environnement :
- `GMAIL_CREDENTIALS_PATH` - Chemin vers le fichier JSON de credentials OAuth2 client
- `GMAIL_TOKEN_CACHE_PATH` - Chemin où sauvegarder le token (optionnel, défaut: `./gmail-token-cache.json`)

## Utilisation

Le reste de l'utilisation reste identique :

```bash
# Mode dry-run
cargo run -- --dry-run

# Traiter 1 email
cargo run -- --limit 1

# Traiter tous les emails
cargo run
```

## Dépannage

### Erreur "PERMISSION_DENIED" ou "Missing access token"
C'est l'erreur la plus courante. Elle signifie que le token OAuth n'a pas les bons scopes.

**Solution** :
1. Supprimez le fichier de cache du token :
   ```bash
   rm gmail-token-cache.json
   ```

2. Vérifiez dans Google Cloud Console que votre OAuth Client a bien les scopes suivants dans l'écran de consentement :
   - `.../auth/gmail.readonly`
   - `.../auth/gmail.modify`  
   - `.../auth/gmail.labels`

3. Relancez l'application - elle vous demandera de vous ré-authentifier avec les nouveaux scopes

### Erreur "Credentials not found"
- Vérifiez que `GMAIL_CREDENTIALS_PATH` pointe vers le bon fichier
- Vérifiez que le fichier JSON est un fichier de credentials OAuth2 Client (pas Service Account)
- Le fichier doit contenir `"installed"` ou `"web"` comme type

### Erreur "Labels not found"
- Assurez-vous que les labels `homemetrics-todo-xsense` et `homemetrics-done-xsense` existent dans Gmail
- Vous pouvez les créer manuellement dans l'interface Gmail (Settings → Labels → Create new label)

### Le navigateur ne s'ouvre pas pour l'authentification
- Vérifiez que votre système peut ouvrir des URLs (commande `xdg-open` sur Linux)
- Si vous êtes en SSH ou dans un container, copiez l'URL affichée et ouvrez-la manuellement dans un navigateur
