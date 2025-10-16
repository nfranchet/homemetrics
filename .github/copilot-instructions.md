<!-- Use this file to provide workspace-specific custom instructions to Copilot. For more details, visit https://code.visualstudio.com/docs/copilot/copilot-customization#_use-a-githubcopilotinstructionsmd-file -->

## Project: HomeMetrics X-Sense Email Client

Ce projet Rust récupère automatiquement les emails de support@x-sense.com, extrait les données de température des pièces jointes, et les sauvegarde dans TimescaleDB.

### Architecture:
- Client IMAP pour récupération d'emails
- Parser de pièces jointes 
- Extracteur de données de température
- Interface TimescaleDB pour persistance

### Étapes de développement:
- [x] Créer workspace Rust
- [ ] Configurer dépendances mail (IMAP, TLS)
- [ ] Configurer dépendances parsing (CSV, JSON, XML)
- [ ] Configurer TimescaleDB (tokio-postgres, sqlx)
- [ ] Implémenter client IMAP
- [ ] Implémenter extraction pièces jointes
- [ ] Implémenter parsing température
- [ ] Implémenter sauvegarde TimescaleDB