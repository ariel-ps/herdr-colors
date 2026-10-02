use rand::{rngs::StdRng, seq::SliceRandom, Rng, SeedableRng};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Debug)]
pub struct Theme {
    pub name: String,
    pub colors: BTreeMap<String, String>,
    lab: [f64; 3],
    cvd: [f64; 3],
}

pub fn valid_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
}

fn rgb(color: &str) -> [f64; 3] {
    [1, 3, 5].map(|i| u8::from_str_radix(&color[i..i + 2], 16).unwrap() as f64)
}

fn linear(color: &str) -> [f64; 3] {
    rgb(color).map(|v| {
        let c = v / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    })
}

fn lab([r, g, b]: [f64; 3]) -> [f64; 3] {
    let xyz = [
        (r * 0.4124 + g * 0.3576 + b * 0.1805) / 0.95047,
        r * 0.2126 + g * 0.7152 + b * 0.0722,
        (r * 0.0193 + g * 0.1192 + b * 0.9505) / 1.08883,
    ];
    let [x, y, z] = xyz.map(|v| {
        if v > (6.0_f64 / 29.0).powi(3) {
            v.cbrt()
        } else {
            v / (3.0 * (6.0_f64 / 29.0).powi(2)) + 4.0 / 29.0
        }
    });
    [116.0 * y - 16.0, 500.0 * (x - y), 200.0 * (y - z)]
}

fn deuteranopia([r, g, b]: [f64; 3]) -> [f64; 3] {
    // Machado et al. full-severity matrix, as used by the original selector.
    lab([
        (0.367322 * r + 0.860646 * g - 0.227968 * b).clamp(0.0, 1.0),
        (0.280085 * r + 0.672501 * g + 0.047413 * b).clamp(0.0, 1.0),
        (-0.011820 * r + 0.042940 * g + 0.968881 * b).clamp(0.0, 1.0),
    ])
}

fn luminance(color: &str) -> f64 {
    let [r, g, b] = linear(color);
    // The older WCAG threshold and the sRGB threshold classify 8-bit channels identically.
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

fn contrast(a: &str, b: &str) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn colorfulness(colors: &BTreeMap<String, String>) -> f64 {
    let pairs: Vec<_> = colors
        .values()
        .map(|c| {
            let [r, g, b] = rgb(c);
            [r - g, 0.5 * (r + g) - b]
        })
        .collect();
    let means = [0, 1].map(|i| pairs.iter().map(|v| v[i]).sum::<f64>() / pairs.len() as f64);
    let variance: f64 = [0, 1]
        .iter()
        .map(|&i| pairs.iter().map(|v| (v[i] - means[i]).powi(2)).sum::<f64>() / pairs.len() as f64)
        .sum();
    variance.sqrt() + 0.3 * means[0].hypot(means[1])
}

impl Theme {
    pub fn parse(name: String, text: &str) -> Result<Self, String> {
        let mut colors = BTreeMap::new();
        for line in text.lines() {
            let parts: Vec<_> = line.split_whitespace().collect();
            let Some(&key) = parts.first() else { continue };
            let ansi = (0..16).any(|i| key == format!("color{i}"));
            if key != "background" && key != "foreground" && !ansi {
                continue;
            }
            if parts.len() != 2 || !valid_color(parts[1]) || colors.contains_key(key) {
                return Err(format!("{name}: invalid or duplicate color: {key}"));
            }
            colors.insert(key.to_owned(), parts[1].to_owned());
        }
        let background = colors
            .get("background")
            .ok_or_else(|| format!("{name}: no background"))?;
        let rgb = linear(background);
        Ok(Self {
            name,
            colors,
            lab: lab(rgb),
            cvd: deuteranopia(rgb),
        })
    }

    fn eligible(&self, max_lightness: f64) -> bool {
        let chroma = self.lab[1].hypot(self.lab[2]);
        self.lab[0] >= 5.0
            && self.lab[0] <= max_lightness
            && (3.0..=25.0).contains(&chroma)
            && self
                .colors
                .get("foreground")
                .is_some_and(|fg| contrast(&self.colors["background"], fg) >= 4.5)
            && colorfulness(&self.colors) <= 150.0
    }

    pub fn payload(&self) -> String {
        let mut output = format!("\\e]11;{}\\e\\\\", self.colors["background"]);
        if let Some(fg) = self.colors.get("foreground") {
            output += &format!("\\e]10;{fg}\\e\\\\");
        }
        for i in 0..16 {
            if let Some(color) = self.colors.get(&format!("color{i}")) {
                output += &format!("\\e]4;{i};{color}\\e\\\\");
            }
        }
        output
    }
}

pub fn load(directory: &Path, max_lightness: f64) -> Result<Vec<Theme>, String> {
    let mut themes = Vec::new();
    let files = fs::read_dir(directory)
        .map_err(|e| format!("{}: {e}. Run herdr-colors sync.", directory.display()))?;
    for entry in files {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().is_none_or(|e| e != "conf") {
            continue;
        }
        let Some(raw_name) = path.file_stem().and_then(|n| n.to_str()) else {
            continue;
        };
        let name = safe_name(raw_name);
        match fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| Theme::parse(name, &text))
        {
            Ok(theme) if theme.eligible(max_lightness) => themes.push(theme),
            Ok(_) => (),
            Err(error) => eprintln!("herdr-colors: skipping {error}"),
        }
    }
    themes.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(themes)
}

fn safe_name(name: &str) -> String {
    let mut safe = String::with_capacity(name.len());
    for character in name.chars() {
        if character.is_control()
            || matches!(
                character,
                '\u{061c}'
                    | '\u{200e}'..='\u{200f}'
                    | '\u{202a}'..='\u{202e}'
                    | '\u{2066}'..='\u{2069}'
            )
        {
            safe.extend(character.escape_default());
        } else {
            safe.push(character);
        }
    }
    safe
}

fn distances(values: &[[f64; 3]]) -> Vec<Vec<f64>> {
    values
        .iter()
        .map(|a| {
            values
                .iter()
                .map(|b| {
                    a.iter()
                        .zip(b)
                        .map(|(a, b)| (a - b).powi(2))
                        .sum::<f64>()
                        .sqrt()
                })
                .collect()
        })
        .collect()
}

fn min_pairwise(genes: &[usize], distances: &[Vec<f64>]) -> f64 {
    let mut minimum = f64::INFINITY;
    for (i, &a) in genes.iter().enumerate() {
        for &b in &genes[i + 1..] {
            minimum = minimum.min(distances[a][b]);
        }
    }
    minimum
}

fn score(genes: &[usize], normal: &[Vec<f64>], cvd: &[Vec<f64>]) -> f64 {
    let safety = min_pairwise(genes, cvd);
    if safety < 10.0 {
        safety - 10.0
    } else {
        min_pairwise(genes, normal)
    }
}

fn tournament(scores: &[f64], rng: &mut StdRng) -> usize {
    let mut candidates = Vec::new();
    while candidates.len() < 3 {
        let index = rng.random_range(0..scores.len());
        if !candidates.contains(&index) {
            candidates.push(index);
        }
    }
    *candidates
        .iter()
        .max_by(|&&a, &&b| scores[a].total_cmp(&scores[b]))
        .unwrap()
}

pub fn select(themes: &[Theme], count: usize, seed: u64) -> Result<Vec<usize>, String> {
    if count == 0 || count > themes.len() {
        return Err(format!(
            "need {count} themes but only {} available after filtering",
            themes.len()
        ));
    }
    let mut rng = StdRng::seed_from_u64(seed);
    if count == 1 {
        return Ok(vec![rng.random_range(0..themes.len())]);
    }
    let normal = distances(&themes.iter().map(|t| t.lab).collect::<Vec<_>>());
    let cvd = distances(&themes.iter().map(|t| t.cvd).collect::<Vec<_>>());
    let mut population: Vec<Vec<usize>> = (0..60)
        .map(|_| {
            let mut genes: Vec<_> = (0..themes.len()).collect();
            genes.shuffle(&mut rng);
            genes.truncate(count);
            genes
        })
        .collect();
    let generations = (1_500_000 / (60 * (count * (count - 1) / 2))).clamp(80, 500);
    let mut best = population[0].clone();
    let mut best_score = f64::NEG_INFINITY;
    for _ in 0..generations {
        let scores: Vec<_> = population
            .iter()
            .map(|genes| score(genes, &normal, &cvd))
            .collect();
        for (genes, &value) in population.iter().zip(&scores) {
            if value > best_score {
                best = genes.clone();
                best_score = value;
            }
        }
        let mut next = vec![best.clone()];
        while next.len() < 60 {
            let mut child = population[tournament(&scores, &mut rng)].clone();
            for &gene in &population[tournament(&scores, &mut rng)] {
                if !child.contains(&gene) {
                    child.push(gene);
                }
            }
            child.shuffle(&mut rng);
            child.truncate(count);
            for i in 0..count {
                if rng.random::<f64>() < 0.15 {
                    let choices: Vec<_> =
                        (0..themes.len()).filter(|g| !child.contains(g)).collect();
                    if !choices.is_empty() {
                        child[i] = choices[rng.random_range(0..choices.len())];
                    }
                }
            }
            next.push(child);
        }
        population = next;
    }
    Ok(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_and_escapes_are_validated() {
        for text in [
            "background #123456\\e]52;bad",
            "background #123456\u{1b}]52;bad",
            "background #123456\nbackground #abcdef",
            "background #123456\ncolor0 #%s",
            "background #123456\nforeground #abc",
        ] {
            assert!(Theme::parse("bad".into(), text).is_err());
        }
        let theme = Theme::parse(
            "good".into(),
            "background #123456\nforeground #abcdef\ncolor0 #ABCDEF",
        )
        .unwrap();
        assert_eq!(
            theme.payload(),
            "\\e]11;#123456\\e\\\\\\e]10;#abcdef\\e\\\\\\e]4;0;#ABCDEF\\e\\\\"
        );
        assert_eq!(safe_name("bad\u{1b}name"), "bad\\u{1b}name");
        assert_eq!(safe_name("bad\u{202e}name"), "bad\\u{202e}name");
    }

    #[test]
    fn metrics_and_selection_keep_the_original_constraints() {
        let white = lab(linear("#ffffff"));
        // Reference values from the former Python selector, including its CVD transform.
        for (actual, expected) in lab(linear("#123456")).iter().zip([
            21.04306195157679,
            1.0588301738765626,
            -24.104716268225335,
        ]) {
            assert!((actual - expected).abs() < 1e-9);
        }
        for (actual, expected) in deuteranopia(linear("#123456")).iter().zip([
            19.93959623280395,
            5.3960664525369015,
            -25.3875240480113,
        ]) {
            assert!((actual - expected).abs() < 1e-9);
        }
        assert!((white[0] - 100.0).abs() < 0.01);
        assert!((contrast("#000000", "#ffffff") - 21.0).abs() < 0.001);
        assert_eq!(lab(linear("#000000")), [0.0, 0.0, 0.0]);
        let themes: Vec<_> = (10..30)
            .map(|v| {
                Theme::parse(
                    format!("theme{v}"),
                    &format!("background #{v:02x}2020\nforeground #ffffff"),
                )
                .unwrap()
            })
            .collect();
        assert!(themes.iter().filter(|t| t.eligible(55.0)).count() >= 16);
        let chosen = select(&themes, 16, 42).unwrap();
        let mut unique = chosen.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 16);
        assert_eq!(chosen, select(&themes, 16, 42).unwrap());
        assert!(select(&themes, 0, 42).is_err());
        assert!(select(&themes, 21, 42).is_err());
        // Gate failures must still produce a solution, even when every score is below -1.
        assert_eq!(select(&themes[..2], 2, 42).unwrap().len(), 2);
    }
}
