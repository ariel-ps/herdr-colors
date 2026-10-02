#!/usr/bin/env python3
# kitty-theme-ga.py — pick N kitty themes out of the full kitty-themes pool
# whose background colors are maximally pairwise-distinct, via a genetic
# algorithm, instead of a fixed hand-picked list. Backs kitty-colorize-panes
# in ../grid.sh.
#
# Fitness is the max-min diversity score: the smallest pairwise CIE Lab
# distance across the chosen set, computed twice — once on normal-vision Lab
# values, once on the same backgrounds run through a deuteranopia simulation
# (Machado, Oliveira & Fernandes 2009) — and combined via min() so a set that
# looks diverse but collapses under the most common form of color vision
# deficiency doesn't score well. A single crowded pair (two near-identical
# dark blues, or two backgrounds that only differ in the red-green channel a
# deuteranope can't see) drags the whole set's score down regardless of how
# spread out everything else is, so the GA is pushed toward sets with no weak
# pair under either model rather than merely a high average distance.
#
# Beyond raw diversity, three filters narrow the candidate pool before the GA
# ever runs, each grounded in a named metric rather than picked by feel:
#   --max-lightness      CIE Lab L*            dark-only pool (unchanged)
#   --max-chroma          CIE Lab chroma        drop neon/high-saturation bgs
#   --min-contrast        WCAG 2.x contrast     drop hard-to-read fg/bg pairs
#   --max-colorfulness     Hasler & Süsstrunk 2003  drop visually "loud" palettes
# The chroma and colorfulness ceilings exist because max-min diversity alone
# will happily pick the most saturated/contrasty themes available (they're
# the easiest way to maximize distance) even though sustained focus favors
# calmer, more muted backgrounds — diversity and calm are different axes and
# the GA only optimizes the first unless the pool is pre-filtered on the
# second.
#
# usage: kitty-theme-ga.py <themes_dir> <count> [--seed N]
# prints the chosen theme names (no .conf suffix), one per line.

import argparse
import glob
import math
import os
import random
import re
import sys

BG_RE = re.compile(r'^background\s+(#[0-9A-Fa-f]{6})\s*$', re.MULTILINE)
FG_RE = re.compile(r'^foreground\s+(#[0-9A-Fa-f]{6})\s*$', re.MULTILINE)
COLOR_RE = re.compile(r'^color\d+\s+(#[0-9A-Fa-f]{6})\s*$', re.MULTILINE)


def srgb_to_linear(c):
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def hex_to_linear_rgb(hexcolor):
    r, g, b = (int(hexcolor[i:i + 2], 16) / 255.0 for i in (1, 3, 5))
    return tuple(srgb_to_linear(c) for c in (r, g, b))


def linear_rgb_to_xyz(r, g, b):
    x = r * 0.4124 + g * 0.3576 + b * 0.1805
    y = r * 0.2126 + g * 0.7152 + b * 0.0722
    z = r * 0.0193 + g * 0.1192 + b * 0.9505
    return x, y, z


def xyz_to_lab(xyz):
    x, y, z = xyz
    xn, yn, zn = 0.95047, 1.0, 1.08883

    def f(t):
        return t ** (1 / 3) if t > (6 / 29) ** 3 else t / (3 * (6 / 29) ** 2) + 4 / 29

    fx, fy, fz = f(x / xn), f(y / yn), f(z / zn)
    return (116 * fy - 16, 500 * (fx - fy), 200 * (fy - fz))


def hex_to_lab(hexcolor):
    return xyz_to_lab(linear_rgb_to_xyz(*hex_to_linear_rgb(hexcolor)))


def chroma(lab):
    return math.hypot(lab[1], lab[2])


# Machado, Oliveira & Fernandes, "A Physiologically-based Model for
# Simulation of Color Vision Deficiency" (2009), full-severity deuteranopia
# matrix. Unlike the older Brettel/Vienot LMS-projection approach, this one
# is fit directly in linear sRGB, so no LMS round-trip is needed here.
DEUTERANOPIA_MATRIX = (
    (0.367322, 0.860646, -0.227968),
    (0.280085, 0.672501, 0.047413),
    (-0.011820, 0.042940, 0.968881),
)


def simulate_deuteranopia_lab(hexcolor):
    r, g, b = hex_to_linear_rgb(hexcolor)
    sim = tuple(
        min(1.0, max(0.0, row[0] * r + row[1] * g + row[2] * b))
        for row in DEUTERANOPIA_MATRIX
    )
    return xyz_to_lab(linear_rgb_to_xyz(*sim))


def wcag_srgb_to_linear(c):
    # WCAG 2.x's own published threshold constant (0.03928, not the 0.04045
    # used above for Lab) — kept as a separate function so contrast-ratio
    # filtering matches what WCAG contrast checkers report, rather than
    # silently drifting if the Lab linearization ever changes.
    return c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4


def rel_luminance(hexcolor):
    r, g, b = (int(hexcolor[i:i + 2], 16) / 255.0 for i in (1, 3, 5))
    r, g, b = (wcag_srgb_to_linear(c) for c in (r, g, b))
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def contrast_ratio(hex1, hex2):
    l1, l2 = rel_luminance(hex1), rel_luminance(hex2)
    lighter, darker = max(l1, l2), min(l1, l2)
    return (lighter + 0.05) / (darker + 0.05)


def colorfulness(hexcolors):
    # Hasler & Süsstrunk, "Measuring Colourfulness in Natural Images" (2003).
    # Computed over the theme's whole palette (background + foreground + ANSI
    # colors), not just the background swatch, since a calm background paired
    # with a neon 16-color syntax palette is still a "loud" theme to work in.
    rg, yb = [], []
    for h in hexcolors:
        r, g, b = (int(h[i:i + 2], 16) for i in (1, 3, 5))
        rg.append(r - g)
        yb.append(0.5 * (r + g) - b)

    def mean(xs):
        return sum(xs) / len(xs)

    def std(xs):
        m = mean(xs)
        return (sum((x - m) ** 2 for x in xs) / len(xs)) ** 0.5

    sigma = math.hypot(std(rg), std(yb))
    mu = math.hypot(abs(mean(rg)), abs(mean(yb)))
    return sigma + 0.3 * mu


def load_themes(themes_dir, max_lightness=None, max_chroma=None,
                 min_contrast=None, max_colorfulness=None,
                 min_lightness=None, min_chroma=None):
    # kitty-themes' backgrounds split cleanly bimodal: dark ones cluster
    # under L*~44, light ones start at L*~77 (nothing in between), so a
    # single lightness cutoff reliably keeps the pool dark without touching
    # the diversity metric itself. Without this, the max-min-distance GA
    # happily proposes a light theme for one pane BECAUSE it's the most
    # distinct background available — technically optimal, unreadable in
    # practice for a terminal grid meant to stay dark. The chroma, contrast,
    # and colorfulness filters below apply the same idea to three other
    # failure modes the diversity metric is blind to.
    themes = {}
    for path in sorted(glob.glob(os.path.join(themes_dir, '*.conf'))):
        try:
            text = open(path, encoding='utf-8', errors='ignore').read()
        except OSError:
            continue
        mbg = BG_RE.search(text)
        if not mbg:
            continue
        bg = mbg.group(1)
        lab = hex_to_lab(bg)
        if max_lightness is not None and lab[0] > max_lightness:
            continue
        # Near-black backgrounds carry ~no hue signal (chroma shrinks toward
        # 0 as L*->0 regardless of hue — it's a gamut-boundary fact, not a
        # measurement artifact) and CIELab's own perceptual-uniformity
        # assumptions break down at the extremes anyway, so a big Lab
        # "distance" between two near-black candidates doesn't mean a human
        # can actually tell them apart. min_lightness/min_chroma keep the GA
        # from picking pool members that only look like "more black".
        if min_lightness is not None and lab[0] < min_lightness:
            continue
        c = chroma(lab)
        if max_chroma is not None and c > max_chroma:
            continue
        if min_chroma is not None and c < min_chroma:
            continue

        mfg = FG_RE.search(text)
        if min_contrast is not None:
            if not mfg or contrast_ratio(bg, mfg.group(1)) < min_contrast:
                continue

        if max_colorfulness is not None:
            palette = [bg] + ([mfg.group(1)] if mfg else []) + COLOR_RE.findall(text)
            if len(palette) >= 2 and colorfulness(palette) > max_colorfulness:
                continue

        name = os.path.splitext(os.path.basename(path))[0]
        themes[name] = (lab, simulate_deuteranopia_lab(bg))
    return themes


def dist(c1, c2):
    return sum((a - b) ** 2 for a, b in zip(c1, c2)) ** 0.5


def build_distance_matrix(lab_values):
    n = len(lab_values)
    dmat = [[0.0] * n for _ in range(n)]
    for i in range(n):
        for j in range(i + 1, n):
            d = dist(lab_values[i], lab_values[j])
            dmat[i][j] = dmat[j][i] = d
    return dmat


def min_pairwise(individual, dmat):
    return min(
        dmat[individual[i]][individual[j]]
        for i in range(len(individual))
        for j in range(i + 1, len(individual))
    )


def fitness(individual, dmat, cvd_dmat, cvd_floor):
    # dmat/cvd_dmat lookups instead of recomputing Lab distance from scratch:
    # this function runs pop_size * generations times, so its per-pair cost
    # is the GA's actual bottleneck. The set of themes doesn't change during
    # a run, so the pairwise distances don't either — computing them once up
    # front turns each fitness call into pure indexing.
    #
    # CVD-safety is a pass/fail gate, NOT an equally-weighted co-objective
    # with min(): a neutral/gray pair carries no hue to lose under CVD
    # simulation, so its cvd distance always equals its normal distance,
    # while a genuinely colorful pair typically loses 15-20% of its distance
    # under simulation (verified empirically). An equal-weight min() combo
    # therefore silently rewards boring/gray candidates over colorful ones
    # every generation, which is how an earlier version of this drifted the
    # whole palette toward near-black/near-gray. Instead: sets that clear the
    # cvd_floor threshold (ΔE76 ~10, the commonly cited "distinguishable to
    # most viewers" line) compete purely on normal-vision diversity; sets
    # that don't clear it are ranked by how badly they fail, so the GA still
    # gets pushed to fix the weak pair, but a passing colorful set is never
    # outcompeted by a passing gray one just for being CVD-safer than it
    # needs to be.
    cvd_min = min_pairwise(individual, cvd_dmat)
    if cvd_min < cvd_floor:
        return cvd_min - cvd_floor
    return min_pairwise(individual, dmat)


def random_individual(pool_size, count, rng):
    return rng.sample(range(pool_size), count)


def crossover(p1, p2, pool_size, count, rng):
    genes = list(dict.fromkeys(p1 + p2))
    rng.shuffle(genes)
    child = genes[:count]
    if len(child) < count:
        remaining = [i for i in range(pool_size) if i not in child]
        rng.shuffle(remaining)
        child += remaining[:count - len(child)]
    return child


def mutate(individual, pool_size, rate, rng):
    individual = list(individual)
    for i in range(len(individual)):
        if rng.random() < rate:
            choices = [g for g in range(pool_size) if g not in individual]
            if choices:
                individual[i] = rng.choice(choices)
    return individual


def tournament(population, scored, k, rng):
    contenders = rng.sample(list(zip(population, scored)), k)
    return max(contenders, key=lambda ps: ps[1])[0]


def run_ga(labs, cvd_labs, count, generations, pop_size, mutation_rate, seed, cvd_floor):
    rng = random.Random(seed)
    names = list(labs.keys())
    lab_values = [labs[n] for n in names]
    cvd_values = [cvd_labs[n] for n in names]
    pool_size = len(names)
    if count > pool_size:
        raise ValueError(f"need {count} themes but only {pool_size} available")
    if count < 2:
        return [names[rng.randrange(pool_size)]] if count == 1 else []

    dmat = build_distance_matrix(lab_values)
    cvd_dmat = build_distance_matrix(cvd_values)
    population = [random_individual(pool_size, count, rng) for _ in range(pop_size)]
    # -inf, not -1.0: a gate-failing individual's score is cvd_min - cvd_floor,
    # which can go well below -1.0 if the whole starting population fails the
    # CVD gate, and best/best_score must still update on generation 0.
    best, best_score = None, float('-inf')

    for _ in range(generations):
        scored = [fitness(ind, dmat, cvd_dmat, cvd_floor) for ind in population]
        for ind, sc in zip(population, scored):
            if sc > best_score:
                best, best_score = ind, sc

        next_gen = [best]
        while len(next_gen) < pop_size:
            p1 = tournament(population, scored, 3, rng)
            p2 = tournament(population, scored, 3, rng)
            child = mutate(crossover(p1, p2, pool_size, count, rng), pool_size, mutation_rate, rng)
            next_gen.append(child)
        population = next_gen

    return [names[i] for i in best]


def auto_generations(count, pop_size):
    # Each generation costs pop_size * C(count, 2) fitness comparisons, so
    # generations alone can't stay a flat constant — a 50-pane tab (1225
    # pairs) at the same generation count as an 11-pane one (55 pairs) would
    # take ~20x longer for a marginal quality gain, moving a colorize call
    # from "instant" to "did it hang?". Scale generations down as pair count
    # grows, aiming for roughly the same total fitness-evaluation budget
    # regardless of pane count, floored so small counts don't get shortchanged
    # and capped so the common case (single-digit/low-teens panes) keeps the
    # full search depth.
    pairs = max(count * (count - 1) // 2, 1)
    budget = 1_500_000
    return max(80, min(500, budget // (pop_size * pairs)))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('themes_dir')
    ap.add_argument('count', type=int)
    ap.add_argument('--seed', type=int, default=None)
    ap.add_argument('--generations', type=int, default=None,
                     help='default: auto-scaled down as pane count grows, see auto_generations()')
    ap.add_argument('--population', type=int, default=60)
    ap.add_argument('--mutation-rate', type=float, default=0.15)
    ap.add_argument('--max-lightness', type=float, default=55.0,
                     help='drop themes with background Lab L* above this (dark-only pool); use a huge value to disable')
    ap.add_argument('--min-lightness', type=float, default=5.0,
                     help='drop themes with background Lab L* below this (near-black backgrounds carry ~no hue signal); use 0 to disable')
    ap.add_argument('--max-chroma', type=float, default=25.0,
                     help='drop themes with background Lab chroma above this (keeps saturated/neon backgrounds out, favors calmer focus); use a huge value to disable')
    ap.add_argument('--min-chroma', type=float, default=3.0,
                     help='drop themes with background Lab chroma below this (near-gray backgrounds carry no hue to differentiate by); use 0 to disable')
    ap.add_argument('--min-contrast', type=float, default=4.5,
                     help='drop themes whose foreground/background WCAG 2.x contrast ratio is below this (keeps text readable); use 0 to disable')
    ap.add_argument('--max-colorfulness', type=float, default=150.0,
                     help='drop themes whose full palette (bg+fg+ANSI colors) Hasler-Susstrunk colourfulness score is above this; use a huge value to disable')
    ap.add_argument('--cvd-floor', type=float, default=10.0,
                     help='minimum pairwise Lab distance required under simulated deuteranopia (ΔE76 ~10 is the commonly cited "distinguishable to most viewers" line); sets below this are penalized rather than treated as an equal-weight objective')
    args = ap.parse_args()

    themes = load_themes(args.themes_dir, args.max_lightness, args.max_chroma,
                          args.min_contrast, args.max_colorfulness,
                          args.min_lightness, args.min_chroma)
    if not themes:
        print(f"kitty-theme-ga: no themes survived filtering in {args.themes_dir}", file=sys.stderr)
        sys.exit(1)
    print(f"kitty-theme-ga: {len(themes)} candidate themes after filtering", file=sys.stderr)

    labs = {name: v[0] for name, v in themes.items()}
    cvd_labs = {name: v[1] for name, v in themes.items()}
    generations = args.generations if args.generations is not None else auto_generations(args.count, args.population)

    try:
        chosen = run_ga(labs, cvd_labs, args.count, generations, args.population,
                         args.mutation_rate, args.seed, args.cvd_floor)
    except ValueError as e:
        print(f"kitty-theme-ga: {e}", file=sys.stderr)
        sys.exit(1)

    for name in chosen:
        print(name)


if __name__ == '__main__':
    main()
