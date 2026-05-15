use chrono::{Local, NaiveDate};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::PathBuf};

#[derive(Parser)]
#[command(name = "plantwatch", about = "Track your houseplants 🌱")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Add a new plant
    Add {
        name: String,
        /// How often to water it (in days)
        #[arg(short, long)]
        every: u32,
    },
    /// Log a watering
    Water { name: String },
    /// Show all active plants and their watering status
    Status,
    /// List all active plant names
    List,
    /// Show full watering history for a plant
    Log { name: String },
    /// Discard a plant (moves it to the graveyard)
    Discard {
        name: String,
        /// How it died or why you removed it
        #[arg(short, long)]
        cause: Option<String>,
    },
    /// Show all discarded plants
    Graveyard,
}

#[derive(Serialize, Deserialize)]
struct Plant {
    frequency_days: u32,
    added_on: NaiveDate,
    watering_log: Vec<NaiveDate>,
}

impl Plant {
    fn last_watered(&self) -> Option<NaiveDate> {
        self.watering_log.last().copied()
    }
}

#[derive(Serialize, Deserialize)]
struct DiscardedPlant {
    frequency_days: u32,
    added_on: NaiveDate,
    discarded_on: NaiveDate,
    cause: Option<String>,
    watering_log: Vec<NaiveDate>,
}

#[derive(Serialize, Deserialize, Default)]
struct Garden {
    active: HashMap<String, Plant>,
    discarded: Vec<(String, DiscardedPlant)>,
}

fn data_path() -> PathBuf {
    dirs::home_dir()
        .expect("could not find home directory")
        .join(".plantwatch.json")
}

fn load() -> Garden {
    let path = data_path();
    if !path.exists() {
        return Garden::default();
    }
    let raw = fs::read_to_string(&path).expect("failed to read data file");
    serde_json::from_str(&raw).unwrap_or_default()
}

fn save(garden: &Garden) {
    let raw = serde_json::to_string_pretty(garden).expect("failed to serialize");
    fs::write(data_path(), raw).expect("failed to write data file");
}

fn main() {
    let cli = Cli::parse();
    let mut garden = load();
    let today = Local::now().date_naive();

    match cli.command {
        Commands::Add { name, every } => {
            if garden.active.contains_key(&name) {
                println!("'{}' is already in your garden.", name);
            } else {
                garden.active.insert(name.clone(), Plant {
                    frequency_days: every,
                    added_on: today,
                    watering_log: vec![],
                });
                save(&garden);
                println!("Added {} (water every {} days).", name, every);
            }
        }

        Commands::Water { name } => {
            match garden.active.get_mut(&name) {
                None => println!("No active plant named '{}'. Add it first.", name),
                Some(plant) => {
                    plant.watering_log.push(today);
                    save(&garden);
                    println!("Watered {}! Good job.", name);
                }
            }
        }

        Commands::Status => {
            if garden.active.is_empty() {
                println!("Your garden is empty. Add a plant with `plantwatch add <name> --every <days>`.");
                return;
            }

            let mut plants: Vec<_> = garden.active.iter().collect();
            plants.sort_by_key(|(name, _)| name.as_str());

            println!("{:<20} {:<12} {:<14} {}", "Plant", "Frequency", "Last Watered", "Status");
            println!("{}", "-".repeat(65));

            for (name, plant) in plants {
                let last = plant.last_watered()
                    .map(|d| d.to_string())
                    .unwrap_or_else(|| "never".into());

                let status = match plant.last_watered() {
                    None => "needs water!".to_string(),
                    Some(d) => {
                        let days_since = (today - d).num_days();
                        let days_left = plant.frequency_days as i64 - days_since;
                        if days_left <= 0 {
                            format!("overdue by {} day(s)!", days_left.abs())
                        } else {
                            format!("ok (water in {} day(s))", days_left)
                        }
                    }
                };

                println!("{:<20} every {:<6} {:<14} {}", name, format!("{}d", plant.frequency_days), last, status);
            }
        }

        Commands::List => {
            if garden.active.is_empty() {
                println!("No active plants.");
                return;
            }
            let mut names: Vec<_> = garden.active.keys().collect();
            names.sort();
            println!("Active plants ({}):", names.len());
            for name in names {
                println!("  - {}", name);
            }
        }

        Commands::Log { name } => {
            match garden.active.get(&name) {
                None => println!("No active plant named '{}'.", name),
                Some(plant) => {
                    println!("Watering log for {} (every {} days):", name, plant.frequency_days);
                    println!("  Added on: {}", plant.added_on);
                    if plant.watering_log.is_empty() {
                        println!("  No waterings recorded yet.");
                    } else {
                        for (i, date) in plant.watering_log.iter().enumerate().rev() {
                            println!("  #{:<4} {}", i + 1, date);
                        }
                    }
                }
            }
        }

        Commands::Discard { name, cause } => {
            match garden.active.remove(&name) {
                None => println!("No active plant named '{}'.", name),
                Some(plant) => {
                    let reason = cause.clone().unwrap_or_else(|| "no reason given".into());
                    garden.discarded.push((name.clone(), DiscardedPlant {
                        frequency_days: plant.frequency_days,
                        added_on: plant.added_on,
                        discarded_on: today,
                        cause,
                        watering_log: plant.watering_log,
                    }));
                    save(&garden);
                    println!("Discarded {}. ({})", name, reason);
                }
            }
        }

        Commands::Graveyard => {
            if garden.discarded.is_empty() {
                println!("No discarded plants.");
                return;
            }
            println!("Discarded plants ({}):", garden.discarded.len());
            println!("{}", "-".repeat(60));
            for (name, plant) in &garden.discarded {
                let cause = plant.cause.as_deref().unwrap_or("no reason given");
                println!("  {} (discarded on {})", name, plant.discarded_on);
                println!("    Added:     {}", plant.added_on);
                println!("    Cause:     {}", cause);
                println!("    Waterings: {}", plant.watering_log.len());
                println!();
            }
        }
    }
}
