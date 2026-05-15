# PlantPal 🌱

A CLI tool for tracking your houseplants. Log waterings, check what's overdue, and get weather-aware reminders for outdoor plants.

## Installation

Make sure you have [Rust installed](https://rustup.rs), then:

```powershell
cd C:\Planted\Planted!
cargo build --release
```

The binary will be at `target/release/plantwatch.exe`. You can copy it anywhere on your PATH to use it as a real command.

During development, use `cargo run --` in place of `plantwatch`.

## Commands

### Add a plant

```
plantwatch add <name> --every <days>
plantwatch add <name> --every <days> --indoor
```

Outdoor is the default. Use `--indoor` for plants that live inside.

```
plantwatch add Monstera --every 7 --indoor
plantwatch add Rose --every 3
```

### Log a watering

```
plantwatch water <name>
```

Records today's date in the plant's watering history.

### Check status

```
plantwatch status
```

Shows all active plants with their watering frequency, last watered date, and whether they are overdue. If a location is configured and it rained enough today, outdoor plants that are overdue will suggest skipping instead.

### List plants

```
plantwatch list
```

Simple list of all active plant names.

### View watering history

```
plantwatch log <name>
```

Shows every recorded watering date for a plant, newest first.

### Discard a plant

```
plantwatch discard <name>
plantwatch discard <name> --cause "dried out"
```

Moves the plant to the graveyard with an optional cause. The full watering history is preserved.

### View the graveyard

```
plantwatch graveyard
```

Shows all discarded plants with their discard date, cause, and total number of waterings.

### Set your location

```
plantwatch setup --lat <latitude> --lon <longitude>
```

Stores your coordinates so `status` can check live rainfall data from [Open-Meteo](https://open-meteo.com). No API key required.

```
plantwatch setup --lat 43.7 --lon -79.4
```

## Weather Integration

When a location is configured, `status` fetches today's precipitation from Open-Meteo before showing the table.

- **2mm or more** — outdoor plants that are overdue show "rained today, skip?" instead of "overdue"
- **Under 2mm** -- light rain warning, still recommends watering
- **Offline or unavailable** -- silently falls back to normal status

Indoor plants are never affected by weather since rain outside does not help them.

## Data Storage

All plant data is saved to `~/.plantwatch.json`. It persists between runs and stores:

- Active plants (name, frequency, indoor/outdoor flag, full watering log)
- Discarded plants (same as above, plus discard date and cause)
- Your saved location (if set)

## Dependencies

| Crate | Purpose |
|---|---|
| clap | CLI argument parsing |
| serde + serde_json | Saving and loading plant data as JSON |
| chrono | Date handling and overdue calculations |
| dirs | Finding the home directory across platforms |
| reqwest | Fetching live weather data from Open-Meteo |
