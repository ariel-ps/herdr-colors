mod palette;

use serde_json::{json, Map, Value};
use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{self, Command},
    thread,
    time::Duration,
};

const HELP: &str = "Herdr Colors — distinct colors for your panes

Usage: herdr-colors COMMAND [OPTIONS]

Commands:
  build [COUNT] [--print] [--max-lightness L] Build palettes (default: 16)
  colorize [PANE ...]                         Repaint panes at a shell prompt
  apply [PANE] [--quiet]                      Emit a pane's cached colors
  detect                                      Describe this pane and its neighbors as JSON
  options [PANE]                              Score every cached palette as JSON
  sync                                        Download themes; prepare missing palettes

Compatibility: herdr-themes-build and herdr-colorize remain available.
Cache: $XDG_CACHE_HOME/herdr-pane-themes (default: ~/.cache/herdr-pane-themes)
Run `herdr-colors COMMAND --help` for command details.
";

const BUILD_HELP: &str = "Usage: herdr-colors build [COUNT] [OPTIONS]

Select and cache COUNT distinct palette slots (default: 16).

Options:
  --print                 Print selected slot and theme names
  --max-lightness L       Maximum CIE Lab L* background lightness (5-100; default: 55)
";

const COLORIZE_HELP: &str = "Usage: herdr-colors colorize [PANE ...]

Repaint the listed Herdr panes, or every pane when none are listed.
Each pane must be at a shell prompt so its terminal can receive the palette.
";

const APPLY_HELP: &str = "Usage: herdr-colors apply [PANE] [--quiet]

Write the pane's raw OSC color sequences to stdout.
PANE defaults to HERDR_PANE_ID. --quiet suppresses error diagnostics.
";

const DETECT_HELP: &str = "Usage: herdr-colors detect

Return JSON describing whether this process is in Herdr, the current terminal pane,
its layout rectangle, and its nearest pane in each cardinal direction.
";

const OPTIONS_HELP: &str = "Usage: herdr-colors options [PANE]

Return every cached palette and its distance from touching panes as JSON.
PANE defaults to HERDR_PANE_ID. The selected slot is applied by apply/colorize.
";

const SYNC_HELP: &str = "Usage: herdr-colors sync

Download or update Kitty themes and build a default palette when the cache is missing or invalid.
Set HERDR_KIT_THEMES to use a local theme directory without Git.
";

type Result<T> = std::result::Result<T, String>;

fn cache_home() -> Result<PathBuf> {
    if let Some(path) = env::var_os("XDG_CACHE_HOME").filter(|v| !v.is_empty()) {
        let path = PathBuf::from(path);
        if path.is_absolute() {
            return Ok(path);
        }
    }
    env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(|p| PathBuf::from(p).join(".cache"))
        .ok_or_else(|| "HOME or XDG_CACHE_HOME must be set".into())
}

fn theme_directory() -> Result<PathBuf> {
    Ok(env::var_os("HERDR_KIT_THEMES")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or(cache_home()?.join("kitty-themes/themes")))
}

fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    let temporary = path.with_extension(format!("{}.tmp", process::id()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(data)?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    let _ = fs::remove_file(&temporary);
    result.map_err(|e| format!("{}: {e}", path.display()))
}

fn build(count: usize, print: bool, max_lightness: f64) -> Result<()> {
    let themes = palette::load(&theme_directory()?, max_lightness)?;
    let selected = palette::select(&themes, count, rand::random())?;
    let cache = cache_home()?.join("herdr-pane-themes");
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let payloads = selected
        .iter()
        .map(|&i| themes[i].payload() + "\n")
        .collect::<String>();
    atomic_write(&cache.join(format!("{count}.txt")), payloads.as_bytes())?;
    atomic_write(&cache.join("current"), format!("{count}\n").as_bytes())?;
    if print {
        for (slot, &i) in selected.iter().enumerate() {
            println!("{}\t{}", slot + 1, themes[i].name);
        }
    }
    eprintln!(
        "herdr-colors: {count} slots -> {}",
        cache.join(format!("{count}.txt")).display()
    );
    Ok(())
}

fn pane_slot(pane: &str) -> Result<usize> {
    let invalid = || format!("invalid pane ID: {}", pane.escape_default());
    if pane.is_empty()
        || pane.split(':').any(str::is_empty)
        || !pane
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'_' | b'-'))
    {
        return Err(invalid());
    }
    let id = pane
        .rsplit(':')
        .next()
        .unwrap_or("")
        .strip_prefix('p')
        .unwrap_or("");
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(invalid());
    }
    usize::from_str_radix(id, 36)
        .ok()
        .and_then(|n| n.checked_sub(1))
        .ok_or_else(invalid)
}

#[derive(Clone)]
struct CachedPalette {
    bytes: Vec<u8>,
    background: String,
}

fn decode_palette(mut text: &str) -> Result<CachedPalette> {
    let mut output = Vec::new();
    let mut seen = Vec::new();
    let mut background = None;
    while !text.is_empty() {
        let (body, rest) = text
            .strip_prefix("\\e]")
            .and_then(|s| s.split_once("\\e\\\\"))
            .ok_or("invalid cached color sequence; run herdr-themes-build")?;
        let parts: Vec<_> = body.split(';').collect();
        let key = match parts.as_slice() {
            [code @ ("10" | "11"), color] if palette::valid_color(color) => code.to_string(),
            ["4", index, color]
                if palette::valid_color(color) && (0..16).any(|i| *index == i.to_string()) =>
            {
                format!("4;{index}")
            }
            _ => return Err("invalid cached color sequence; run herdr-themes-build".into()),
        };
        if seen.contains(&key) {
            return Err("duplicate cached color sequence".into());
        }
        if key == "11" {
            background = Some(parts[1].to_owned());
        }
        seen.push(key);
        output.extend_from_slice(format!("\x1b]{body}\x1b\\").as_bytes());
        text = rest;
    }
    Ok(CachedPalette {
        bytes: output,
        background: background.ok_or("cached palette has no background")?,
    })
}

fn cached_palettes() -> Result<Vec<CachedPalette>> {
    let directory = cache_home()?.join("herdr-pane-themes");
    let missing = || "no cached palette. Run herdr-themes-build, then herdr-colorize.".to_string();
    let count = fs::read_to_string(directory.join("current"))
        .map_err(|_| missing())?
        .trim()
        .parse::<usize>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(missing)?;
    let text = fs::read_to_string(directory.join(format!("{count}.txt"))).map_err(|_| missing())?;
    let lines: Vec<_> = text.lines().collect();
    if lines.len() != count {
        return Err(missing());
    }
    lines.into_iter().map(decode_palette).collect()
}

fn run(command: &mut Command) -> Result<String> {
    let output = command
        .output()
        .map_err(|e| format!("{}: {e}", command.get_program().to_string_lossy()))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "{} failed ({}): {}",
            command.get_program().to_string_lossy(),
            output.status,
            detail.trim()
        ));
    }
    String::from_utf8(output.stdout).map_err(|e| e.to_string())
}

fn herdr(args: &[&str]) -> Result<Value> {
    let output = run(Command::new(
        env::var_os("HERDR_BIN_PATH")
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "herdr".into()),
    )
    .args(args))?;
    let value: Value =
        serde_json::from_str(&output).map_err(|e| format!("invalid Herdr response: {e}"))?;
    if let Some(error) = value.get("error") {
        return Err(format!("Herdr: {error}"));
    }
    value
        .get("result")
        .cloned()
        .ok_or_else(|| "Herdr response has no result".into())
}

fn herdr_environment() -> bool {
    env::var("HERDR_ENV").is_ok_and(|value| value == "1")
}

fn no_pane(in_herdr: bool) -> Value {
    json!({
        "in_herdr": in_herdr,
        "has_pane": false,
        "pane": null,
        "location": null,
        "area": null,
        "neighbors": {
            "left": null,
            "right": null,
            "up": null,
            "down": null
        }
    })
}

fn rectangle_values(value: Option<&Value>, name: &str) -> Result<[u16; 4]> {
    let value = value
        .and_then(Value::as_object)
        .ok_or_else(|| format!("Herdr {name} is not a rectangle"))?;
    let number = |field: &str, positive: bool| {
        value
            .get(field)
            .and_then(Value::as_u64)
            .and_then(|number| u16::try_from(number).ok())
            .filter(|number| !positive || *number > 0)
            .ok_or_else(|| format!("Herdr {name}.{field} is not valid"))
    };
    Ok([
        number("x", false)?,
        number("y", false)?,
        number("width", true)?,
        number("height", true)?,
    ])
}

fn rectangle(value: Option<&Value>, name: &str) -> Result<Value> {
    rectangle_values(value, name)?;
    value
        .cloned()
        .ok_or_else(|| format!("Herdr {name} is not a rectangle"))
}

fn pane_rectangle(layout: &Map<String, Value>, pane_id: &str, name: &str) -> Result<Value> {
    let panes = layout
        .get("panes")
        .and_then(Value::as_array)
        .ok_or("Herdr pane layout has no panes")?;
    let pane = panes
        .iter()
        .find(|candidate| candidate["pane_id"].as_str() == Some(pane_id))
        .ok_or_else(|| format!("{name} is missing from Herdr layout"))?;
    rectangle(pane.get("rect"), &format!("{name} location"))
}

fn detect() -> Result<Value> {
    let in_herdr = herdr_environment();
    if !in_herdr {
        return Ok(no_pane(false));
    }
    let pane_id = match env::var("HERDR_PANE_ID") {
        Ok(value) if !value.is_empty() => value,
        Ok(_) | Err(env::VarError::NotPresent) => return Ok(no_pane(true)),
        Err(env::VarError::NotUnicode(_)) => return Err("HERDR_PANE_ID is not valid UTF-8".into()),
    };
    detect_pane(&pane_id)
}

fn detect_pane(pane_id: &str) -> Result<Value> {
    pane_slot(pane_id)?;

    let current = herdr(&["pane", "current", "--pane", pane_id])?;
    let pane = current
        .get("pane")
        .and_then(Value::as_object)
        .ok_or("Herdr response has no current pane object")?;
    if pane.get("pane_id").and_then(Value::as_str) != Some(pane_id) {
        return Err("Herdr current pane ID does not match HERDR_PANE_ID".into());
    }
    if pane
        .get("terminal_id")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        return Err("Herdr current pane has no terminal ID".into());
    }
    let pane = Value::Object(pane.clone());

    let layout_result = herdr(&["pane", "layout", "--pane", pane_id])?;
    let layout = layout_result
        .get("layout")
        .and_then(Value::as_object)
        .ok_or("Herdr response has no pane layout object")?;
    let location = pane_rectangle(layout, pane_id, "current pane")?;
    let area = rectangle(layout.get("area"), "layout area")?;

    let mut neighbors = Map::new();
    for direction in ["left", "right", "up", "down"] {
        let result = herdr(&[
            "pane",
            "neighbor",
            "--pane",
            pane_id,
            "--direction",
            direction,
        ])?;
        let neighbor = result
            .get("neighbor")
            .and_then(Value::as_object)
            .ok_or("Herdr response has no neighbor object")?;
        if neighbor.get("pane_id").and_then(Value::as_str) != Some(pane_id)
            || neighbor.get("direction").and_then(Value::as_str) != Some(direction)
        {
            return Err(format!(
                "Herdr returned mismatched {direction} neighbor data"
            ));
        }
        let neighbor = match neighbor.get("neighbor_pane_id") {
            None | Some(Value::Null) => Value::Null,
            Some(value) => {
                let neighbor_id = value
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| format!("Herdr {direction} neighbor has an invalid pane ID"))?;
                pane_slot(neighbor_id)?;
                let neighbor_layout = neighbor
                    .get("layout")
                    .and_then(Value::as_object)
                    .ok_or_else(|| format!("Herdr {direction} neighbor has no layout object"))?;
                json!({
                    "pane_id": neighbor_id,
                    "location": pane_rectangle(
                        neighbor_layout,
                        neighbor_id,
                        &format!("{direction} neighbor")
                    )?
                })
            }
        };
        neighbors.insert(direction.to_owned(), neighbor);
    }

    Ok(json!({
        "in_herdr": true,
        "has_pane": true,
        "pane": pane,
        "location": location,
        "area": area,
        "neighbors": neighbors
    }))
}

fn write_json(value: &Value) -> Result<()> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    serde_json::to_writer(&mut stdout, value).map_err(|error| error.to_string())?;
    writeln!(stdout).map_err(|error| error.to_string())
}

fn detect_command() -> Result<()> {
    match detect() {
        Ok(value) => write_json(&value),
        Err(error) => {
            write_json(&json!({
                "in_herdr": herdr_environment(),
                "has_pane": env::var("HERDR_PANE_ID").is_ok_and(|value| !value.is_empty()),
                "error": error
            }))?;
            Err(String::new())
        }
    }
}

#[derive(Clone, Copy)]
struct PaneRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

struct LayoutPane {
    id: String,
    order: usize,
    rect: PaneRect,
}

struct Recommendation {
    slot: usize,
    json: Value,
}

fn touching(a: PaneRect, b: PaneRect) -> bool {
    let horizontal_overlap = a.x < b.x + b.width && b.x < a.x + a.width;
    let vertical_overlap = a.y < b.y + b.height && b.y < a.y + a.height;
    ((a.x + a.width == b.x || b.x + b.width == a.x) && vertical_overlap)
        || ((a.y + a.height == b.y || b.y + b.height == a.y) && horizontal_overlap)
}

fn assignment_seed(panes: &[LayoutPane]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for pane in panes {
        for byte in pane
            .id
            .bytes()
            .chain(pane.rect.x.to_le_bytes())
            .chain(pane.rect.y.to_le_bytes())
            .chain(pane.rect.width.to_le_bytes())
            .chain(pane.rect.height.to_le_bytes())
        {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash
}

fn layout_panes(pane_id: &str) -> Result<Vec<LayoutPane>> {
    let result = herdr(&["pane", "layout", "--pane", pane_id])?;
    let panes = result["layout"]["panes"]
        .as_array()
        .ok_or("Herdr pane layout has no panes")?;
    let mut output: Vec<LayoutPane> = Vec::with_capacity(panes.len());
    for pane in panes {
        let id = pane["pane_id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or("Herdr pane layout contains an invalid pane ID")?;
        if output.iter().any(|existing| existing.id == id) {
            return Err(format!("Herdr pane layout contains duplicate pane ID {id}"));
        }
        let order = pane_slot(id)?;
        let [x, y, width, height] = rectangle_values(pane.get("rect"), &format!("{id} location"))?;
        output.push(LayoutPane {
            id: id.to_owned(),
            order,
            rect: PaneRect {
                x: x.into(),
                y: y.into(),
                width: width.into(),
                height: height.into(),
            },
        });
    }
    output.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.id.cmp(&b.id)));
    if !output.iter().any(|pane| pane.id == pane_id) {
        return Err("current pane is missing from Herdr layout".into());
    }
    Ok(output)
}

fn recommend(pane_id: &str, palettes: &[CachedPalette]) -> Result<Recommendation> {
    if palettes.is_empty() {
        return Err("no cached palettes".into());
    }
    let panes = layout_panes(pane_id)?;
    let edges = (0..panes.len())
        .flat_map(|a| {
            let panes = &panes;
            (a + 1..panes.len())
                .filter(move |&b| touching(panes[a].rect, panes[b].rect))
                .map(move |b| (a, b))
        })
        .collect::<Vec<_>>();
    let backgrounds = palettes
        .iter()
        .map(|palette| palette.background.clone())
        .collect::<Vec<_>>();
    let base = panes
        .iter()
        .map(|pane| pane.order % palettes.len())
        .collect::<Vec<_>>();
    let assigned = palette::assign(
        &backgrounds,
        panes.len(),
        &edges,
        &base,
        assignment_seed(&panes),
    )?;
    let current = panes
        .iter()
        .position(|pane| pane.id == pane_id)
        .ok_or("current pane is missing from assignment")?;
    let neighbor_indexes = edges
        .iter()
        .filter_map(|&(a, b)| {
            if a == current {
                Some(b)
            } else if b == current {
                Some(a)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let mut options = Vec::with_capacity(palettes.len());
    for (slot, candidate) in palettes.iter().enumerate() {
        let mut min_normal = f64::INFINITY;
        let mut min_cvd = f64::INFINITY;
        for &neighbor in &neighbor_indexes {
            let neighbor_slot = assigned[neighbor];
            let (normal, cvd) = palette::background_distances(
                &candidate.background,
                &palettes[neighbor_slot].background,
            )?;
            min_normal = min_normal.min(normal);
            min_cvd = min_cvd.min(cvd);
        }
        let (normal, cvd, score) = if neighbor_indexes.is_empty() {
            (Value::Null, Value::Null, Value::Null)
        } else {
            let score = if min_cvd < palette::MIN_CVD_DISTANCE {
                min_cvd - palette::MIN_CVD_DISTANCE
            } else {
                min_normal
            };
            (json!(min_normal), json!(min_cvd), json!(score))
        };
        options.push(json!({
            "slot": slot + 1,
            "background": candidate.background,
            "minimum_distance": normal,
            "minimum_deuteranopia_distance": cvd,
            "score": score,
            "selected": slot == assigned[current]
        }));
    }
    let assignment = panes
        .iter()
        .enumerate()
        .map(|(index, pane)| {
            json!({
                "pane_id": pane.id,
                "slot": assigned[index] + 1,
                "background": palettes[assigned[index]].background
            })
        })
        .collect::<Vec<_>>();
    let neighbors = neighbor_indexes
        .iter()
        .map(|&index| {
            json!({
                "pane_id": panes[index].id,
                "slot": assigned[index] + 1,
                "background": palettes[assigned[index]].background
            })
        })
        .collect::<Vec<_>>();

    Ok(Recommendation {
        slot: assigned[current],
        json: json!({
            "pane_id": pane_id,
            "strategy": "genetic-layout",
            "selected_slot": assigned[current] + 1,
            "selected_background": palettes[assigned[current]].background,
            "touching_neighbors": neighbors,
            "assignment": assignment,
            "options": options
        }),
    })
}

fn options_command(pane: Option<&str>) -> Result<()> {
    let result: Result<Value> = (|| {
        let pane = pane
            .map(str::to_owned)
            .or_else(|| {
                env::var("HERDR_PANE_ID")
                    .ok()
                    .filter(|value| !value.is_empty())
            })
            .ok_or("options requires a pane ID or HERDR_PANE_ID")?;
        pane_slot(&pane)?;
        let palettes = cached_palettes()?;
        Ok(recommend(&pane, &palettes)?.json)
    })();
    match result {
        Ok(value) => write_json(&value),
        Err(error) => {
            write_json(&json!({"error": error}))?;
            Err(String::new())
        }
    }
}

fn location_aware_payload(pane: &str, enabled: bool) -> Result<(Vec<u8>, Option<String>)> {
    let base = pane_slot(pane)?;
    let palettes = cached_palettes()?;
    let fallback = base % palettes.len();
    if !enabled {
        return Ok((palettes[fallback].bytes.clone(), None));
    }
    match recommend(pane, &palettes) {
        Ok(recommendation) => Ok((palettes[recommendation.slot].bytes.clone(), None)),
        Err(error) => Ok((
            palettes[fallback].bytes.clone(),
            Some(format!("location-aware selection unavailable: {error}")),
        )),
    }
}

fn shell_pid(pane: &str) -> Result<u64> {
    let info = herdr(&["pane", "process-info", "--pane", pane])?;
    info["process_info"]["shell_pid"]
        .as_u64()
        .filter(|pid| *pid > 0)
        .ok_or_else(|| "no shell PID".into())
}

fn terminal_for_pid(pid: u64) -> Result<String> {
    let tty = run(Command::new("ps").args(["-o", "tty=", "-p", &pid.to_string()]))?;
    let tty = tty.trim().to_owned();
    let valid = tty
        .strip_prefix("pts/")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        || (tty.starts_with("tty") && tty.bytes().all(|b| b.is_ascii_alphanumeric()));
    if !valid {
        return Err("no usable terminal for pane".into());
    }
    Ok(tty)
}

fn repaint(pane: &str) -> Result<()> {
    let (bytes, warning) = location_aware_payload(pane, true)?;
    if let Some(warning) = warning {
        eprintln!("{}: {warning}", pane.escape_default());
    }
    let pid = shell_pid(pane)?;
    let tty = terminal_for_pid(pid)?;
    let path = Path::new("/dev").join(&tty);
    // Holding the device open prevents it from being reused after verification.
    let mut terminal = fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if shell_pid(pane)? != pid || terminal_for_pid(pid)? != tty {
        return Err("pane terminal changed during repaint".into());
    }
    terminal.write_all(&bytes).map_err(|e| e.to_string())?;
    println!("{pane} -> {}", path.display());
    Ok(())
}

fn colorize(mut panes: Vec<String>) -> Result<()> {
    if panes.is_empty() {
        let result = herdr(&["pane", "list"])?;
        let list = result["panes"]
            .as_array()
            .ok_or("Herdr response has no pane list")?;
        panes = list
            .iter()
            .map(|p| {
                p["pane_id"]
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "Herdr pane has no ID".to_string())
            })
            .collect::<Result<_>>()?;
    }
    let mut failures = 0;
    for pane in panes {
        if let Err(error) = repaint(&pane) {
            eprintln!("{}: {error}", pane.escape_default());
            failures += 1;
        }
    }
    if failures == 0 {
        Ok(())
    } else {
        Err(format!("could not repaint {failures} panes"))
    }
}

fn update_themes(directory: &Path) -> Result<()> {
    for attempt in 0..20 {
        match run(Command::new("git")
            .env("LC_ALL", "C")
            .arg("-C")
            .arg(directory)
            .args(["pull", "--ff-only"]))
        {
            Ok(_) => return Ok(()),
            Err(error)
                if attempt < 19
                    && ["index.lock", "cannot lock ref", "another git process"]
                        .iter()
                        .any(|marker| error.to_ascii_lowercase().contains(marker)) =>
            {
                thread::sleep(Duration::from_millis(50));
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!()
}

fn sync() -> Result<()> {
    if env::var_os("HERDR_KIT_THEMES")
        .filter(|v| !v.is_empty())
        .is_none()
    {
        let directory = cache_home()?.join("kitty-themes");
        if directory.join(".git").is_dir() {
            update_themes(&directory)?;
        } else {
            let parent = directory
                .parent()
                .ok_or_else(|| format!("{} has no parent", directory.display()))?;
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            let temporary = parent.join(format!(
                ".kitty-themes.clone.{}.{}",
                process::id(),
                rand::random::<u64>()
            ));
            let clone = run(Command::new("git")
                .args([
                    "clone",
                    "--depth",
                    "1",
                    "https://github.com/kovidgoyal/kitty-themes.git",
                ])
                .arg(&temporary));
            if let Err(error) = clone {
                let _ = fs::remove_dir_all(&temporary);
                return Err(error);
            }
            if let Err(error) = fs::rename(&temporary, &directory) {
                let won_by_other_sync = directory.join(".git").is_dir();
                let _ = fs::remove_dir_all(&temporary);
                if !won_by_other_sync {
                    return Err(format!("{}: {error}", directory.display()));
                }
            }
        }
        eprintln!("herdr-colors: themes synchronized");
    }
    if cached_palettes().is_err() {
        build(16, false, 55.0)?;
    }
    Ok(())
}

fn execute(args: Vec<String>) -> std::result::Result<(), (i32, String)> {
    let invalid = |message: &str| (2, message.to_owned());
    if args.is_empty() || matches!(args[0].as_str(), "-h" | "--help") {
        print!("{HELP}");
        return Ok(());
    }
    if args.len() == 2 && matches!(args[1].as_str(), "-h" | "--help") {
        let help = match args[0].as_str() {
            "build" => BUILD_HELP,
            "colorize" => COLORIZE_HELP,
            "apply" => APPLY_HELP,
            "detect" => DETECT_HELP,
            "options" => OPTIONS_HELP,
            "sync" => SYNC_HELP,
            _ => {
                return Err(invalid(
                    "unknown command or extra arguments. Run herdr-colors --help.",
                ))
            }
        };
        print!("{help}");
        return Ok(());
    }
    let result = match args[0].as_str() {
        "build" => {
            let mut count = None;
            let mut print = false;
            let mut max_lightness = 55.0;
            let mut values = args[1..].iter();
            while let Some(arg) = values.next() {
                match arg.as_str() {
                    "--print" => print = true,
                    "--max-lightness" => {
                        max_lightness = values
                            .next()
                            .and_then(|v| v.parse::<f64>().ok())
                            .filter(|v| v.is_finite() && (5.0..=100.0).contains(v))
                            .ok_or_else(|| {
                                invalid("--max-lightness requires a number from 5 to 100")
                            })?;
                    }
                    _ if count.is_none() && !arg.starts_with('-') => {
                        count = Some(
                            arg.parse::<usize>()
                                .ok()
                                .filter(|n| *n > 0)
                                .ok_or_else(|| invalid("count must be a positive integer"))?,
                        );
                    }
                    _ => return Err(invalid(&format!("unexpected argument: {arg}"))),
                }
            }
            build(count.unwrap_or(16), print, max_lightness)
        }
        "colorize" => {
            if args[1..].iter().any(|s| s.starts_with('-')) {
                return Err(invalid("colorize expects pane IDs"));
            }
            colorize(args[1..].to_vec())
        }
        "apply" => {
            let mut quiet = false;
            let mut pane = None;
            for arg in &args[1..] {
                if arg == "--quiet" {
                    quiet = true;
                } else if pane.is_none() && !arg.starts_with('-') {
                    pane = Some(arg.clone());
                } else {
                    return Err(invalid("apply expects one pane ID and optional --quiet"));
                }
            }
            let pane = pane
                .or_else(|| env::var("HERDR_PANE_ID").ok())
                .unwrap_or_default();
            let result =
                location_aware_payload(&pane, herdr_environment()).and_then(|(data, warning)| {
                    if !quiet {
                        if let Some(warning) = warning {
                            eprintln!("herdr-colors: {warning}");
                        }
                    }
                    io::stdout().write_all(&data).map_err(|e| e.to_string())
                });
            if quiet {
                return result.map_err(|_| (1, String::new()));
            }
            result
        }
        "detect" if args.len() == 1 => detect_command(),
        "options" => {
            if args.len() > 2 || args.get(1).is_some_and(|pane| pane.starts_with('-')) {
                return Err(invalid("options expects at most one pane ID"));
            }
            options_command(args.get(1).map(String::as_str))
        }
        "sync" if args.len() == 1 => sync(),
        _ => {
            return Err(invalid(
                "unknown command or extra arguments. Run herdr-colors --help.",
            ))
        }
    };
    result.map_err(|error| (1, error))
}

fn main() {
    let args = env::args_os()
        .skip(1)
        .map(|arg| {
            arg.into_string()
                .map_err(|_| "arguments must be valid UTF-8".to_string())
        })
        .collect::<Result<Vec<_>>>();
    let result = args.map_err(|error| (2, error)).and_then(execute);
    if let Err((code, error)) = result {
        if !error.is_empty() {
            eprintln!("herdr-colors: {error}");
        }
        process::exit(code);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_ids_and_legacy_cache_are_validated() {
        assert_eq!(pane_slot("w1:pA").unwrap(), 9);
        assert_eq!(pane_slot("p10").unwrap(), 35);
        for id in [
            "",
            "p0",
            "../bad",
            "p-1",
            "\u{1b}[31m:p1",
            "w1::p1",
            "p99999999999999999999999999999999999",
        ] {
            let error = pane_slot(id).unwrap_err();
            assert!(!error.contains('\u{1b}'));
        }
        assert_eq!(
            decode_palette("\\e]11;#123456\\e\\\\").unwrap().bytes,
            b"\x1b]11;#123456\x1b\\"
        );
        for text in [
            "",
            "\\e]52;#123456\\e\\\\",
            "\\e]11;#123456%s\\e\\\\",
            "\\e]4;99;#123456\\e\\\\",
        ] {
            assert!(decode_palette(text).is_err());
        }
    }
}
