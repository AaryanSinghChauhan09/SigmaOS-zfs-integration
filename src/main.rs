//! CLI for sigmaos-zfs-integration.

use clap::{Parser, Subcommand};
use sigmaos_zfs_integration::{default_state_path, Dataset, ZfsState};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "sigmaos-zfs-integration", about = "SigmaOS ZFS integration")]
struct Cli {
    #[arg(long, global = true)]
    state: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List datasets.
    List,
    /// List datasets in a pool prefix.
    Pool { prefix: String },
    /// List snapshot-enabled datasets.
    Snapshots,
    /// Add a dataset record.
    Add {
        name: String,
        #[arg(long, default_value = "filesystem")]
        kind: String,
        #[arg(long, default_value = "")]
        mountpoint: String,
        #[arg(long, default_value = "lz4")]
        compression: String,
    },
    /// Remove a dataset record.
    Remove { name: String },
    /// Set scrub cron schedule.
    Scrub { cron: String },
    /// Validate state file.
    Validate,
    /// Print state path.
    Path,
}

fn state_path(s: &Option<PathBuf>) -> Result<PathBuf, Box<dyn std::error::Error>> {
    s.clone()
        .or_else(default_state_path)
        .ok_or_else(|| "cannot determine state path".into())
}

fn load(s: &Option<PathBuf>) -> Result<ZfsState, Box<dyn std::error::Error>> {
    let p = state_path(s)?;
    if p.exists() {
        Ok(ZfsState::load(&p)?)
    } else {
        let d = ZfsState::default();
        d.save(&p)?;
        Ok(d)
    }
}

fn save(st: &ZfsState, path: &Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    st.save(&state_path(path)?)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let cli = Cli::parse();
    let mut st = load(&cli.state)?;
    match cli.command {
        Commands::List => {
            for d in st.datasets.values() {
                println!("{} [{}] {}@{}", d.name, d.kind, d.compression, d.mountpoint);
            }
        }
        Commands::Pool { prefix } => {
            for d in st.by_pool(&prefix) {
                println!("{}", d.name);
            }
        }
        Commands::Snapshots => {
            for d in st.snapshot_targets() {
                println!("{} (retention {})", d.name, d.snapshot_retention);
            }
        }
        Commands::Add {
            name,
            kind,
            mountpoint,
            compression,
        } => {
            st.add_dataset(Dataset {
                name,
                kind,
                mountpoint,
                compression,
                snapshots_enabled: true,
                snapshot_retention: 8,
            })?;
            save(&st, &cli.state)?;
        }
        Commands::Remove { name } => {
            st.remove_dataset(&name)?;
            save(&st, &cli.state)?;
            println!("removed {name}");
        }
        Commands::Scrub { cron } => {
            st.scrub = Some(sigmaos_zfs_integration::ScrubSchedule {
                cron,
                datasets: Vec::new(),
            });
            save(&st, &cli.state)?;
            println!("scrub schedule updated");
        }
        Commands::Validate => {
            st.validate()?;
            println!("valid");
        }
        Commands::Path => println!("{}", state_path(&cli.state)?.display()),
    }
    Ok(())
}
