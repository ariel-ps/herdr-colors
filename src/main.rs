mod palette;

use serde_json::Value;
use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{self, Command},
};

const HELP: &str = "Herdr Colors — distinct colors for your panes

Usage: herdr-colors COMMAND [OPTIONS]

Commands:
  build [COUNT] [--print] [--max-lightness L]   Build palettes (default: 16)
  colorize [PANE ...]                         Repaint panes at a shell prompt
  apply [PANE] [--quiet]                      Emit a pane's cached colors
  sync                                       Download themes; prepare missing palettes

Compatibility: herdr-themes-build and herdr-colorize remain available.
Cache: $XDG_CACHE_HOME/herdr-pane-themes (default: ~/.cache/herdr-pane-themes)
";

type Result<T> = std::result::Result<T, String>;

fn cache_home() -> Result<PathBuf> {
    if let Some(path) = env::var_os("XDG_CACHE_HOME").filter(|v| !v.is_empty()) {
        return Ok(path.into());
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

fn decode_payload(mut text: &str) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut seen = Vec::new();
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
        seen.push(key);
        output.extend_from_slice(format!("\x1b]{body}\x1b\\").as_bytes());
        text = rest;
    }
    if !seen.iter().any(|s| s == "11") {
        return Err("cached palette has no background".into());
    }
    Ok(output)
}

fn payload(pane: &str) -> Result<Vec<u8>> {
    let slot = pane_slot(pane)?;
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
    decode_payload(lines[slot % count])
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
        env::var_os("HERDR_BIN_PATH").unwrap_or_else(|| "herdr".into()),
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

fn repaint(pane: &str) -> Result<()> {
    let bytes = payload(pane)?;
    let info = herdr(&["pane", "process-info", "--pane", pane])?;
    let pid = info["process_info"]["shell_pid"]
        .as_u64()
        .filter(|pid| *pid > 0)
        .ok_or("no shell PID")?;
    let tty = run(Command::new("ps").args(["-o", "tty=", "-p", &pid.to_string()]))?;
    let tty = tty.trim();
    let valid = tty
        .strip_prefix("pts/")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        || (tty.starts_with("tty") && tty.bytes().all(|b| b.is_ascii_alphanumeric()));
    if !valid {
        return Err("no usable terminal for pane".into());
    }
    let path = Path::new("/dev").join(tty);
    // Never create a file when a pane exits between process lookup and writing.
    let mut terminal = fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
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

fn sync() -> Result<()> {
    if env::var_os("HERDR_KIT_THEMES")
        .filter(|v| !v.is_empty())
        .is_none()
    {
        let directory = cache_home()?.join("kitty-themes");
        if directory.join(".git").is_dir() {
            if let Err(error) = run(Command::new("git")
                .arg("-C")
                .arg(&directory)
                .args(["pull", "--ff-only"]))
            {
                eprintln!("herdr-colors: keeping existing themes: {error}");
            }
        } else {
            fs::create_dir_all(directory.parent().unwrap()).map_err(|e| e.to_string())?;
            run(Command::new("git")
                .args([
                    "clone",
                    "--depth",
                    "1",
                    "https://github.com/kovidgoyal/kitty-themes.git",
                ])
                .arg(&directory))?;
        }
    }
    if payload("p1").is_err() {
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
        print!("{HELP}");
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
                            .filter(|v| v.is_finite() && *v >= 0.0)
                            .ok_or_else(|| {
                                invalid("--max-lightness requires a finite nonnegative number")
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
            let result = payload(&pane)
                .and_then(|data| io::stdout().write_all(&data).map_err(|e| e.to_string()));
            if quiet {
                return result.map_err(|_| (1, String::new()));
            }
            result
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
    if let Err((code, error)) = execute(env::args().skip(1).collect()) {
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
            decode_payload("\\e]11;#123456\\e\\\\").unwrap(),
            b"\x1b]11;#123456\x1b\\"
        );
        for text in [
            "",
            "\\e]52;#123456\\e\\\\",
            "\\e]11;#123456%s\\e\\\\",
            "\\e]4;99;#123456\\e\\\\",
        ] {
            assert!(decode_payload(text).is_err());
        }
    }
}
