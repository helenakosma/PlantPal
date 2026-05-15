use chrono::{Local, NaiveDate};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::PathBuf};

#[derive(Parser)]
#[command(name = "plantpal", about = "Track your houseplants 🌱")]
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
        /// Mark as an indoor plant (weather won't affect watering suggestions)
        #[arg(long)]
        indoor: bool,
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
    /// Set your location for weather-based watering suggestions
    Setup {
        #[arg(short, long)]
        lat: f64,
        #[arg(short, long)]
        lon: f64,
    },
}

#[derive(Serialize, Deserialize)]
struct Plant {
    frequency_days: u32,
    added_on: NaiveDate,
    watering_log: Vec<NaiveDate>,
    indoor: bool,
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
    location: Option<(f64, f64)>,
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

// Returns today's precipitation in mm, or None if unavailable
fn fetch_precipitation(lat: f64, lon: f64) -> Option<f64> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&daily=precipitation_sum&timezone=auto&forecast_days=1",
        lat, lon
    );
    let resp: serde_json::Value = reqwest::blocking::get(&url).ok()?.json().ok()?;
    resp["daily"]["precipitation_sum"][0].as_f64()
}

fn main() {
    let cli = Cli::parse();
    let mut garden = load();
    let today = Local::now().date_naive();

    match cli.command {
        Commands::Setup { lat, lon } => {
            garden.location = Some((lat, lon));
            save(&garden);
            println!("Location set to ({}, {}).", lat, lon);
            println!("Status will now check rainfall before suggesting waterings.");
        }

        Commands::Add { name, every, indoor } => {
            if garden.active.contains_key(&name) {
                println!("'{}' is already in your garden.", name);
            } else {
                let location = if indoor { "indoor" } else { "outdoor" };
                garden.active.insert(name.clone(), Plant {
                    frequency_days: every,
                    added_on: today,
                    watering_log: vec![],
                    indoor,
                });
                save(&garden);
                println!("Added {} ({}, water every {} days).", name, location, every);
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

            // Fetch today's rainfall if location is configured
            let rain_mm = garden.location.and_then(|(lat, lon)| {
                print!("Checking weather... ");
                let result = fetch_precipitation(lat, lon);
                if result.is_none() {
                    println!("unavailable (offline?)");
                }
                result
            });

            if let Some(mm) = rain_mm {
                if mm >= 2.0 {
                    println!("🌧️  It rained {:.1}mm today — outdoor plants likely don't need watering.\n", mm);
                } else if mm > 0.0 {
                    println!("🌦️  Light rain today ({:.1}mm) — probably not enough to skip watering.\n", mm);
                } else {
                    println!("☀️  No rain today ({:.1}mm).\n", mm);
                }
            }

            let rained_enough = rain_mm.map(|mm| mm >= 2.0).unwrap_or(false);

            let mut plants: Vec<_> = garden.active.iter().collect();
            plants.sort_by_key(|(name, _)| name.as_str());

            println!("{:<20} {:<10} {:<12} {:<14} {}", "Plant", "Location", "Frequency", "Last Watered", "Status");
            println!("{}", "-".repeat(75));

            for (name, plant) in plants {
                let last = plant.last_watered()
                    .map(|d| d.to_string())
                    .unwrap_or_else(|| "never".into());

                let apply_rain = rained_enough && !plant.indoor;

                let status = match plant.last_watered() {
                    None => {
                        if apply_rain {
                            format!("rained {:.1}mm — skip?", rain_mm.unwrap())
                        } else {
                            "needs water!".to_string()
                        }
                    }
                    Some(d) => {
                        let days_since = (today - d).num_days();
                        let days_left = plant.frequency_days as i64 - days_since;
                        if days_left <= 0 {
                            if apply_rain {
                                format!("overdue, but rained {:.1}mm — skip?", rain_mm.unwrap())
                            } else {
                                format!("overdue by {} day(s)!", days_left.abs())
                            }
                        } else {
                            format!("ok (water in {} day(s))", days_left)
                        }
                    }
                };

                let location = if plant.indoor { "indoor" } else { "outdoor" };
                println!("{:<20} {:<10} every {:<6} {:<14} {}", name, location, format!("{}d", plant.frequency_days), last, status);
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
