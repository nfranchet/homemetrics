use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub imap: ImapConfig,
    pub database: DatabaseConfig,
    pub data_dir: String,
}

#[derive(Debug, Deserialize)]
pub struct ImapConfig {
    pub server: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub mailbox: String,
}

#[derive(Debug, Deserialize)]
pub struct DatabaseConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
}

impl Config {
    pub fn new() -> Result<Self> {
        // Vérifier que les variables essentielles sont définies
        Self::check_required_env_vars()?;
        
        // Configuration chargée depuis les variables d'environnement
        Ok(Config {
            imap: ImapConfig {
                server: std::env::var("IMAP_SERVER")
                    .unwrap_or_else(|_| "imap.gmail.com".to_string()),
                port: std::env::var("IMAP_PORT")
                    .unwrap_or_else(|_| "993".to_string())
                    .parse()
                    .unwrap_or(993),
                username: std::env::var("IMAP_USERNAME")
                    .expect("IMAP_USERNAME doit être défini"),
                password: std::env::var("IMAP_PASSWORD")
                    .expect("IMAP_PASSWORD doit être défini"),
                mailbox: std::env::var("IMAP_MAILBOX")
                    .unwrap_or_else(|_| "INBOX".to_string()),
            },
            database: DatabaseConfig {
                host: std::env::var("DB_HOST")
                    .unwrap_or_else(|_| "localhost".to_string()),
                port: std::env::var("DB_PORT")
                    .unwrap_or_else(|_| "5432".to_string())
                    .parse()
                    .unwrap_or(5432),
                database: std::env::var("DB_NAME")
                    .unwrap_or_else(|_| "homemetrics".to_string()),
                username: std::env::var("DB_USERNAME")
                    .unwrap_or_else(|_| "postgres".to_string()),
                password: std::env::var("DB_PASSWORD")
                    .expect("DB_PASSWORD doit être défini"),
            },
            data_dir: std::env::var("DATA_DIR")
                .unwrap_or_else(|_| "./data".to_string()),
        })
    }
    
    fn check_required_env_vars() -> Result<()> {
        let required_vars = [
            "IMAP_USERNAME",
            "IMAP_PASSWORD",
        ];
        
        let mut missing_vars = Vec::new();
        
        for var in &required_vars {
            if std::env::var(var).is_err() {
                missing_vars.push(*var);
            }
        }
        
        if !missing_vars.is_empty() {
            anyhow::bail!(
                "Variables d'environnement manquantes: {}\n\
                 \n\
                 💡 Solutions :\n\
                 1. Créer un fichier .env avec vos credentials :\n\
                    cp .env.example .env\n\
                    # Puis éditer .env avec vos valeurs\n\
                 \n\
                 2. Ou définir les variables manuellement :\n\
                    export IMAP_USERNAME=your-email@gmail.com\n\
                    export IMAP_PASSWORD=your-app-password\n\
                    cargo run -- --dry-run\n\
                 \n\
                 3. Voir le README.md pour plus d'informations",
                missing_vars.join(", ")
            );
        }
        
        Ok(())
    }
}