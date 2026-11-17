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

impl Garden {
    /// Returns false if a plant with that name already exists.
    fn add_plant(&mut self, name: &str, every: u32, indoor: bool, today: NaiveDate) -> bool {
        if self.active.contains_key(name) {
            return false;
        }
        self.active.insert(name.to_string(), Plant {
            frequency_days: every,
            added_on: today,
            watering_log: vec![],
            indoor,
        });
        true
    }

    /// Returns false if no such active plant exists.
    fn water_plant(&mut self, name: &str, today: NaiveDate) -> bool {
        match self.active.get_mut(name) {
            Some(plant) => {
                plant.watering_log.push(today);
                true
            }
            None => false,
        }
    }

    /// Moves a plant to the graveyard. Returns the reason shown, or None if no such plant.
    fn discard_plant(&mut self, name: &str, cause: Option<String>, today: NaiveDate) -> Option<String> {
        let plant = self.active.remove(name)?;
        let reason = cause.clone().unwrap_or_else(|| "no reason given".into());
        self.discarded.push((name.to_string(), DiscardedPlant {
            frequency_days: plant.frequency_days,
            added_on: plant.added_on,
            discarded_on: today,
            cause,
            watering_log: plant.watering_log,
        }));
        Some(reason)
    }
}

/// Rainfall (mm) at or above which overdue outdoor plants are told to skip watering.
const RAIN_SKIP_MM: f64 = 2.0;

fn rained_enough(rain_mm: Option<f64>) -> bool {
    rain_mm.map(|mm| mm >= RAIN_SKIP_MM).unwrap_or(false)
}

fn watering_status(plant: &Plant, today: NaiveDate, rain_mm: Option<f64>) -> String {
    let apply_rain = rained_enough(rain_mm) && !plant.indoor;
    match plant.last_watered() {
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
    }
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
            if garden.add_plant(&name, every, indoor, today) {
                save(&garden);
                let location = if indoor { "indoor" } else { "outdoor" };
                println!("Added {} ({}, water every {} days).", name, location, every);
            } else {
                println!("'{}' is already in your garden.", name);
            }
        }

        Commands::Water { name } => {
            if garden.water_plant(&name, today) {
                save(&garden);
                println!("Watered {}! Good job.", name);
            } else {
                println!("No active plant named '{}'. Add it first.", name);
            }
        }

        Commands::Status => {
            if garden.active.is_empty() {
                println!("Your garden is empty. Add a plant with `plantpal add <name> --every <days>`.");
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
                if mm >= RAIN_SKIP_MM {
                    println!("🌧️  It rained {:.1}mm today — outdoor plants likely don't need watering.\n", mm);
                } else if mm > 0.0 {
                    println!("🌦️  Light rain today ({:.1}mm) — probably not enough to skip watering.\n", mm);
                } else {
                    println!("☀️  No rain today ({:.1}mm).\n", mm);
                }
            }

            let mut plants: Vec<_> = garden.active.iter().collect();
            plants.sort_by_key(|(name, _)| name.as_str());

            println!("{:<20} {:<10} {:<12} {:<14} {}", "Plant", "Location", "Frequency", "Last Watered", "Status");
            println!("{}", "-".repeat(75));

            for (name, plant) in plants {
                let last = plant.last_watered()
                    .map(|d| d.to_string())
                    .unwrap_or_else(|| "never".into());

                let status = watering_status(plant, today, rain_mm);

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
            match garden.discard_plant(&name, cause, today) {
                None => println!("No active plant named '{}'.", name),
                Some(reason) => {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn plant(freq: u32, indoor: bool, log: Vec<NaiveDate>) -> Plant {
        Plant { frequency_days: freq, added_on: d(2026, 1, 1), watering_log: log, indoor }
    }

    // --- rain rule ---

    #[test]
    fn rain_threshold_is_2mm_inclusive() {
        assert!(rained_enough(Some(2.0)));
        assert!(rained_enough(Some(10.0)));
        assert!(!rained_enough(Some(1.9)));
        assert!(!rained_enough(Some(0.0)));
    }

    #[test]
    fn no_weather_data_never_counts_as_rain() {
        assert!(!rained_enough(None));
    }

    // --- status ---

    #[test]
    fn never_watered_plant_needs_water() {
        let p = plant(7, false, vec![]);
        assert_eq!(watering_status(&p, d(2026, 6, 1), None), "needs water!");
    }

    #[test]
    fn recently_watered_plant_is_ok() {
        let p = plant(7, true, vec![d(2026, 6, 1)]);
        assert_eq!(watering_status(&p, d(2026, 6, 3), None), "ok (water in 5 day(s))");
    }

    #[test]
    fn plant_due_today_counts_as_overdue_by_zero() {
        let p = plant(7, true, vec![d(2026, 6, 1)]);
        assert_eq!(watering_status(&p, d(2026, 6, 8), None), "overdue by 0 day(s)!");
    }

    #[test]
    fn overdue_plant_reports_days_late() {
        let p = plant(3, true, vec![d(2026, 6, 1)]);
        assert_eq!(watering_status(&p, d(2026, 6, 6), None), "overdue by 2 day(s)!");
    }

    #[test]
    fn overdue_outdoor_plant_skips_when_it_rained_enough() {
        let p = plant(3, false, vec![d(2026, 6, 1)]);
        assert_eq!(
            watering_status(&p, d(2026, 6, 6), Some(2.5)),
            "overdue, but rained 2.5mm — skip?"
        );
    }

    #[test]
    fn overdue_outdoor_plant_still_overdue_in_light_rain() {
        let p = plant(3, false, vec![d(2026, 6, 1)]);
        assert_eq!(watering_status(&p, d(2026, 6, 6), Some(1.9)), "overdue by 2 day(s)!");
    }

    #[test]
    fn indoor_plant_ignores_rain() {
        let p = plant(3, true, vec![d(2026, 6, 1)]);
        assert_eq!(watering_status(&p, d(2026, 6, 6), Some(15.0)), "overdue by 2 day(s)!");
    }

    #[test]
    fn never_watered_outdoor_plant_skips_when_it_rained_enough() {
        let p = plant(3, false, vec![]);
        assert_eq!(watering_status(&p, d(2026, 6, 6), Some(4.0)), "rained 4.0mm — skip?");
    }

    #[test]
    fn rain_does_not_hide_a_plant_that_is_not_due() {
        let p = plant(7, false, vec![d(2026, 6, 5)]);
        assert_eq!(watering_status(&p, d(2026, 6, 6), Some(8.0)), "ok (water in 6 day(s))");
    }

    // --- garden operations ---

    #[test]
    fn add_plant_stores_settings() {
        let mut g = Garden::default();
        assert!(g.add_plant("Monstera", 7, true, d(2026, 6, 1)));
        let p = &g.active["Monstera"];
        assert_eq!(p.frequency_days, 7);
        assert!(p.indoor);
        assert_eq!(p.added_on, d(2026, 6, 1));
        assert!(p.watering_log.is_empty());
    }

    #[test]
    fn add_plant_rejects_duplicates_without_overwriting() {
        let mut g = Garden::default();
        g.add_plant("Rose", 3, false, d(2026, 6, 1));
        assert!(!g.add_plant("Rose", 9, true, d(2026, 6, 2)));
        assert_eq!(g.active["Rose"].frequency_days, 3);
    }

    #[test]
    fn water_plant_appends_to_log_and_updates_last_watered() {
        let mut g = Garden::default();
        g.add_plant("Rose", 3, false, d(2026, 6, 1));
        assert!(g.water_plant("Rose", d(2026, 6, 2)));
        assert!(g.water_plant("Rose", d(2026, 6, 5)));
        let p = &g.active["Rose"];
        assert_eq!(p.watering_log, vec![d(2026, 6, 2), d(2026, 6, 5)]);
        assert_eq!(p.last_watered(), Some(d(2026, 6, 5)));
    }

    #[test]
    fn water_unknown_plant_fails() {
        let mut g = Garden::default();
        assert!(!g.water_plant("Ghost", d(2026, 6, 1)));
    }

    #[test]
    fn discard_moves_plant_to_graveyard_keeping_history() {
        let mut g = Garden::default();
        g.add_plant("Fern", 4, true, d(2026, 6, 1));
        g.water_plant("Fern", d(2026, 6, 2));
        let reason = g.discard_plant("Fern", Some("dried out".into()), d(2026, 6, 20));
        assert_eq!(reason.as_deref(), Some("dried out"));
        assert!(!g.active.contains_key("Fern"));
        assert_eq!(g.discarded.len(), 1);
        let (name, dead) = &g.discarded[0];
        assert_eq!(name, "Fern");
        assert_eq!(dead.discarded_on, d(2026, 6, 20));
        assert_eq!(dead.cause.as_deref(), Some("dried out"));
        assert_eq!(dead.watering_log, vec![d(2026, 6, 2)]);
    }

    #[test]
    fn discard_without_cause_uses_default_reason() {
        let mut g = Garden::default();
        g.add_plant("Fern", 4, true, d(2026, 6, 1));
        assert_eq!(g.discard_plant("Fern", None, d(2026, 6, 2)).as_deref(), Some("no reason given"));
    }

    #[test]
    fn discard_unknown_plant_returns_none() {
        let mut g = Garden::default();
        assert!(g.discard_plant("Ghost", None, d(2026, 6, 1)).is_none());
        assert!(g.discarded.is_empty());
    }

    #[test]
    fn garden_survives_json_roundtrip() {
        let mut g = Garden::default();
        g.add_plant("Rose", 3, false, d(2026, 6, 1));
        g.water_plant("Rose", d(2026, 6, 2));
        g.add_plant("Fern", 4, true, d(2026, 6, 1));
        g.discard_plant("Fern", Some("pests".into()), d(2026, 6, 9));
        g.location = Some((43.7, -79.4));

        let back: Garden = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        assert_eq!(back.active["Rose"].watering_log, vec![d(2026, 6, 2)]);
        assert_eq!(back.discarded.len(), 1);
        assert_eq!(back.location, Some((43.7, -79.4)));
    }
}
