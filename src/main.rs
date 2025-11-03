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
#[command(about = "HomeMetrics mail client to retrieve X-Sense data")]
#[command(version = "0.1.0")]
struct Args {
    /// Dry-run mode: analyze emails without saving to database
    #[arg(short, long)]
    dry_run: bool,
    
    /// Daemon mode: run the program as a daemon with scheduling
    #[arg(long)]
    daemon: bool,
    
    /// Attachment save directory (default: ./data)
    #[arg(short = 'o', long, default_value = "./data")]
    data_dir: String,
    
    /// Limit the number of emails to process (default: unlimited)
    #[arg(short = 'l', long)]
    limit: Option<usize>,
    
    /// Check configuration without connecting
    #[arg(long)]
    check_config: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file if it exists
    dotenv::dotenv().ok();
    
    // Parse CLI arguments
    let args = Args::parse();
    
    // Initialize logging
    env_logger::init();
    
    if args.dry_run {
        info!("🧪 Starting HomeMetrics X-Sense mail client in DRY-RUN mode");
    } else {
        info!("🚀 Starting HomeMetrics X-Sense mail client");
    }
    
    // Load configuration
    let mut config = Config::new()?;
    
    // If requested, only check configuration
    if args.check_config {
        println!("✅ Configuration valide !");
        println!("📧 Gmail API OAuth2");
        println!("🔑 Credentials: {}", config.gmail.credentials_path);
        println!("💾 Token cache: {}", config.gmail.token_cache_path);
        println!("📁 Data directory: {}", config.data_dir);
        if !args.dry_run {
            println!("🗄️  Database: {}@{}:{}/{}", 
                     config.database.username, config.database.host, 
                     config.database.port, config.database.database);
        }
        return Ok(());
    }
    
    // Override data_dir with CLI argument if provided
    if args.data_dir != "./data" {
        config.data_dir = args.data_dir.clone();
    }
    
    // If daemon mode is enabled
    if args.daemon {
        info!("🔄 Starting in daemon mode");
        run_daemon_mode(config, args).await?;
        return Ok(());
    }
    
    // One-shot mode (default behavior)
    let result = if args.dry_run {
        // Dry-run mode: no database connection
        let processor = EmailProcessor::new_dry_run(config)?;
        processor.process_emails_dry_run(args.limit).await
    } else {
        // Production mode: with database
        let mut processor = EmailProcessor::new(config).await?;
        processor.process_emails(args.limit).await
    };
    
    match result {
        Ok(count) => {
            if args.dry_run {
                info!("✅ Dry-run analysis completed successfully. {} emails analyzed.", count);
            } else {
                info!("✅ Processing completed successfully. {} emails processed.", count);
            }
        }
        Err(e) => {
            error!("❌ Error processing emails: {}", e);
            return Err(e);
        }
    }
    
    Ok(())
}

async fn run_daemon_mode(config: Config, args: Args) -> Result<()> {
    use tokio_cron_scheduler::{JobScheduler, Job};
    use chrono::{Local, Timelike};
    
    // Check that the scheduler is enabled in configuration
    if !config.scheduler.enabled {
        error!("❌ Daemon mode requires SCHEDULER_ENABLED=true in configuration");
        anyhow::bail!("Scheduler not enabled in configuration");
    }
    
    if config.scheduler.schedule_times.is_empty() {
        error!("❌ No scheduling times defined (SCHEDULER_TIMES)");
        anyhow::bail!("No scheduling times defined");
    }
    
    info!("📅 Configured retrieval times: {:?}", config.scheduler.schedule_times);
    
    // Create the scheduler
    let scheduler = JobScheduler::new().await?;
    
    // Add a job for each configured time
    for schedule_time in &config.scheduler.schedule_times {
        let parts: Vec<&str> = schedule_time.split(':').collect();
        if parts.len() != 2 {
            error!("❌ Invalid time format: {}. Use HH:MM format", schedule_time);
            continue;
        }
        
        let hour = parts[0];
        let minute = parts[1];
        
        // Cron format: "0 minute hour * * *" (every day)
        let cron_expr = format!("0 {} {} * * *", minute, hour);
        info!("📆 Adding scheduled job: {} (cron: {})", schedule_time, cron_expr);
        
        // Clone variables needed for the closure
        let config_clone = config.clone();
        let dry_run = args.dry_run;
        let limit = args.limit;
        let schedule_time_clone = schedule_time.clone();
        
        let job = Job::new_async(cron_expr.as_str(), move |_uuid, _l| {
            let config = config_clone.clone();
            let schedule_time = schedule_time_clone.clone();
            
            Box::pin(async move {
                info!("⏰ Scheduled execution at {} - Retrieving emails...", schedule_time);
                
                let result = if dry_run {
                    let processor = match EmailProcessor::new_dry_run(config) {
                        Ok(p) => p,
                        Err(e) => {
                            error!("❌ Error creating processor: {}", e);
                            return;
                        }
                    };
                    processor.process_emails_dry_run(limit).await
                } else {
                    let mut processor = match EmailProcessor::new(config).await {
                        Ok(p) => p,
                        Err(e) => {
                            error!("❌ Error creating processor: {}", e);
                            return;
                        }
                    };
                    processor.process_emails(limit).await
                };
                
                match result {
                    Ok(count) => {
                        info!("✅ Scheduled processing completed. {} emails processed at {}", count, schedule_time);
                    }
                    Err(e) => {
                        error!("❌ Error during scheduled processing at {}: {}", schedule_time, e);
                    }
                }
            })
        })?;
        
        scheduler.add(job).await?;
    }
    
    // Start the scheduler
    scheduler.start().await?;
    
    info!("✅ Daemon mode started. Waiting for scheduled times...");
    info!("📋 Next executions: {:?}", config.scheduler.schedule_times);
    info!("⏸️  Press Ctrl+C to stop the daemon");
    
    // Keep the program alive
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        
        // Periodic log to show the daemon is active
        let now = Local::now();
        if now.minute() == 0 {
            info!("💓 Daemon active - {}", now.format("%Y-%m-%d %H:%M"));
        }
    }
}
