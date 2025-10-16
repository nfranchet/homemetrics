use anyhow::Result;
use log::{info, error};
use clap::Parser;

mod config;
mod imap_client;
mod attachment_parser;
mod temperature_extractor;
mod database;
mod email_processor;

use config::Config;
use email_processor::EmailProcessor;

#[derive(Parser)]
#[command(name = "homemetrics")]
#[command(about = "Client mail HomeMetrics pour récupérer les données X-Sense")]
#[command(version = "0.1.0")]
struct Args {
    /// Mode dry-run : analyse les emails sans sauvegarde en base de données
    #[arg(short, long)]
    dry_run: bool,
    
    /// Répertoire de sauvegarde des pièces jointes (par défaut: ./data)
    #[arg(short = 'o', long, default_value = "./data")]
    data_dir: String,
    
    /// Limite du nombre d'emails à traiter (par défaut: illimité)
    #[arg(short = 'l', long)]
    limit: Option<usize>,
    
    /// Vérifier la configuration sans se connecter
    #[arg(long)]
    check_config: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Charger le fichier .env s'il existe
    dotenv::dotenv().ok();
    
    // Parser les arguments CLI
    let args = Args::parse();
    
    // Initialiser le logging
    env_logger::init();
    
    if args.dry_run {
        info!("🧪 Démarrage en mode DRY-RUN du client mail HomeMetrics X-Sense");
    } else {
        info!("🚀 Démarrage du client mail HomeMetrics X-Sense");
    }
    
    // Charger la configuration
    let mut config = Config::new()?;
    
    // Si demandé, vérifier seulement la configuration
    if args.check_config {
        println!("✅ Configuration valide !");
        println!("📧 Serveur IMAP: {}:{}", config.imap.server, config.imap.port);
        println!("👤 Utilisateur: {}", config.imap.username);
        println!("📁 Répertoire data: {}", config.data_dir);
        if !args.dry_run {
            println!("🗄️  Base de données: {}@{}:{}/{}", 
                     config.database.username, config.database.host, 
                     config.database.port, config.database.database);
        }
        return Ok(());
    }
    
    // Remplacer le data_dir par celui des arguments CLI s'il est fourni
    if args.data_dir != "./data" {
        config.data_dir = args.data_dir;
    }
    
    // Lancer le traitement selon le mode
    let result = if args.dry_run {
        // Mode dry-run : pas de connexion base de données
        let processor = EmailProcessor::new_dry_run(config)?;
        processor.process_emails_dry_run(args.limit).await
    } else {
        // Mode production : avec base de données
        let mut processor = EmailProcessor::new(config).await?;
        processor.process_emails(args.limit).await
    };
    
    match result {
        Ok(count) => {
            if args.dry_run {
                info!("✅ Analyse dry-run terminée avec succès. {} emails analysés.", count);
            } else {
                info!("✅ Traitement terminé avec succès. {} emails traités.", count);
            }
        }
        Err(e) => {
            error!("❌ Erreur lors du traitement des emails: {}", e);
            return Err(e);
        }
    }
    
    Ok(())
}
