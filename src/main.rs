use anyhow::Result;
use log::{info, error};
use clap::Parser;

mod config;
mod gmail_client;
mod attachment_parser;
mod temperature_extractor;
mod database;
mod email_processor;
mod slack_notifier;

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
    
    /// Mode daemon : lance le programme en mode daemon avec scheduling
    #[arg(long)]
    daemon: bool,
    
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
        println!("📧 Gmail API OAuth2");
        println!("🔑 Credentials: {}", config.gmail.credentials_path);
        println!("💾 Token cache: {}", config.gmail.token_cache_path);
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
        config.data_dir = args.data_dir.clone();
    }
    
    // Si le mode daemon est activé
    if args.daemon {
        info!("🔄 Démarrage en mode daemon");
        run_daemon_mode(config, args).await?;
        return Ok(());
    }
    
    // Mode one-shot (comportement par défaut)
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

async fn run_daemon_mode(config: Config, args: Args) -> Result<()> {
    use tokio_cron_scheduler::{JobScheduler, Job};
    use chrono::{Local, Timelike};
    
    // Vérifier que le scheduler est activé dans la configuration
    if !config.scheduler.enabled {
        error!("❌ Le mode daemon nécessite SCHEDULER_ENABLED=true dans la configuration");
        anyhow::bail!("Scheduler non activé dans la configuration");
    }
    
    if config.scheduler.schedule_times.is_empty() {
        error!("❌ Aucun horaire de scheduling défini (SCHEDULER_TIMES)");
        anyhow::bail!("Aucun horaire de scheduling défini");
    }
    
    info!("📅 Horaires de récupération configurés : {:?}", config.scheduler.schedule_times);
    
    // Créer le scheduler
    let scheduler = JobScheduler::new().await?;
    
    // Ajouter un job pour chaque horaire configuré
    for schedule_time in &config.scheduler.schedule_times {
        let parts: Vec<&str> = schedule_time.split(':').collect();
        if parts.len() != 2 {
            error!("❌ Format d'horaire invalide: {}. Utilisez le format HH:MM", schedule_time);
            continue;
        }
        
        let hour = parts[0];
        let minute = parts[1];
        
        // Format cron: "0 minute hour * * *" (tous les jours)
        let cron_expr = format!("0 {} {} * * *", minute, hour);
        info!("📆 Ajout du job planifié : {} (cron: {})", schedule_time, cron_expr);
        
        // Cloner les variables nécessaires pour le closure
        let config_clone = config.clone();
        let dry_run = args.dry_run;
        let limit = args.limit;
        let schedule_time_clone = schedule_time.clone();
        
        let job = Job::new_async(cron_expr.as_str(), move |_uuid, _l| {
            let config = config_clone.clone();
            let schedule_time = schedule_time_clone.clone();
            
            Box::pin(async move {
                info!("⏰ Exécution planifiée à {} - Récupération des emails...", schedule_time);
                
                let result = if dry_run {
                    let processor = match EmailProcessor::new_dry_run(config) {
                        Ok(p) => p,
                        Err(e) => {
                            error!("❌ Erreur lors de la création du processeur: {}", e);
                            return;
                        }
                    };
                    processor.process_emails_dry_run(limit).await
                } else {
                    let mut processor = match EmailProcessor::new(config).await {
                        Ok(p) => p,
                        Err(e) => {
                            error!("❌ Erreur lors de la création du processeur: {}", e);
                            return;
                        }
                    };
                    processor.process_emails(limit).await
                };
                
                match result {
                    Ok(count) => {
                        info!("✅ Traitement planifié terminé. {} emails traités à {}", count, schedule_time);
                    }
                    Err(e) => {
                        error!("❌ Erreur lors du traitement planifié à {}: {}", schedule_time, e);
                    }
                }
            })
        })?;
        
        scheduler.add(job).await?;
    }
    
    // Démarrer le scheduler
    scheduler.start().await?;
    
    info!("✅ Mode daemon démarré. En attente des horaires planifiés...");
    info!("📋 Prochaines exécutions : {:?}", config.scheduler.schedule_times);
    info!("⏸️  Appuyez sur Ctrl+C pour arrêter le daemon");
    
    // Garder le programme en vie
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        
        // Log périodique pour montrer que le daemon est actif
        let now = Local::now();
        if now.minute() == 0 {
            info!("💓 Daemon actif - {}", now.format("%Y-%m-%d %H:%M"));
        }
    }
}
