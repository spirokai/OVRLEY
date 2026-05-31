# Theme Plan

## Scope and assumptions

- This plan is only about visual style, composition, typography, and widget treatment.
- Heading is intentionally excluded.
- Scene constraints remain fixed across all themes: 3840x2160, global scale 1, shadows capped at 4, border thickness capped at 2.
- Allowed fonts: Bebas Neue, Evogria, Furore, Saira Stencil, Teko, Inter Black, Rajdhani, Oxanium, Share Tech Mono, JetBrains Mono.
- The missing 16th theme is added as `Graphite Motorsport`.

## Style coverage goals

- Include themes with and without route.
- Include themes with and without elevation.
- Include elevation with area on and area off.
- Vary completed vs remaining line color and opacity.
- Keep most themes without shadows, but include a few with controlled glow/drop-shadow use.
- Include themes with and without the gradient widget.
- Keep most themes without text borders, but include a few with thin borders.
- Include themes with and without icons.
- Use built-in unit color in some themes and text widgets as labels/units in others.
- Include one deliberate mixed-font theme where values and labels use different fonts.
- Vary density: sparse hero layouts, broadcast stacks, modular dashboards, and lower-third ribbons.

## Themes

### 1. SAFA White Hero

- Palette: pure white with a faint cool-gray accent only for units and remaining path.
- Typography: Evogria everywhere.
- Composition: spacious hero composition; title text top-right, primary metric block on the right, supporting metrics stacked on the left, full-width elevation across the bottom, small route map floating upper-right.
- Widget mix: route, elevation, time, speed, power, heartrate, cadence, gradient, title text.
- Visual treatment: no shadows, no borders, icons only on the left-side support metrics, route completed in solid white and remaining in cool gray at lower opacity, elevation area on with very soft white fill, metric elevation label only.
- Distinguishing idea: this is the clean premium flagship theme, driven by scale and negative space rather than density.

### 2. SAFA White Technical Ribbon

- Palette: pure white with steel-gray secondary text and a slightly darker border.
- Typography: Evogria for values and text.
- Composition: dense lower-third ribbon; most widgets aligned along the bottom third with a narrow elevation strip running edge to edge.
- Widget mix: elevation, speed, power, heartrate, cadence, temperature, gradient, text widgets used as labels above the metrics.
- Visual treatment: no route, elevation area off, no icons, 1px border on text, very small marker, thin line weight, both metric and imperial elevation labels turned on, gradient kept compact and integrated into the ribbon.
- Distinguishing idea: this is the "broadcast telemetry strip" white theme, proving that white can also work in a dense technical layout.

### 3. SAFA White Route Focus

- Palette: pure white with cool-gray remaining route and a pale mint unit color.
- Typography: Evogria.
- Composition: large route map occupies the right half, three oversized metrics sit bottom-left, title text is minimal or absent.
- Widget mix: route, speed, power, time, temperature.
- Visual treatment: no elevation, no gradient, icons on for secondary metrics, route line thicker than Theme 1, larger route marker, no border, subtle shadow on the route marker only via global shadow settings kept very low.
- Distinguishing idea: this theme exists to showcase route geometry as the hero element instead of the elevation profile.

### 4. SAFA White Modular Grid

- Palette: pure white with cool-gray labels and a slightly darker white border.
- Typography: Evogria for values, built-in units kept white or hidden.
- Composition: modular dashboard grid; six to eight widgets arranged in a clean matrix rather than a stack.
- Widget mix: route, elevation, speed, heartrate, power, cadence, text widgets used as units below the two primary metrics.
- Visual treatment: no icons, no shadows, elevation area off, route and elevation both compact, marker sizes reduced, line weight medium, no elevation labels.
- Distinguishing idea: this is the most structured white theme and should feel engineered rather than cinematic.

### 5. Neutral White Editorial

- Palette: pure white with soft gray units, labels, and remaining path.
- Typography: Teko.
- Composition: asymmetrical editorial layout; left column of metrics, small route bottom-right, generous empty center.
- Widget mix: route, speed, time, power, temperature, one small text title.
- Visual treatment: no elevation, no gradient, no icons, no shadows, route completed and remaining both white but separated mostly by opacity, values slightly reduced in opacity for a softer feel.
- Distinguishing idea: a restrained non-Evogria white option that feels more like magazine typography than a HUD.

### 6. Neutral White Broadcast Minimal

- Palette: pure white with off-white units and faint gray labels.
- Typography: Rajdhani.
- Composition: simple vertical stack in the lower-left, long narrow elevation at the bottom, nothing in the upper third except optional small title text.
- Widget mix: elevation, speed, heartrate, power, time, gradient, text widgets used as labels above each metric.
- Visual treatment: route absent, elevation area on but extremely light, icons off, no borders, no shadows, metric elevation label only, decimals enabled only where they materially help readability.
- Distinguishing idea: a newsroom-style clean overlay that reads fast and wastes no space.

### 7. Neutral White Mixed-Font Ledger

- Palette: pure white with pale gray labels and muted aqua units.
- Typography: Oxanium for values, Share Tech Mono for text labels. This is the one intentional mixed-font theme.
- Composition: ledger-like left column with precise alignment; small map in the upper-right to avoid overloading the layout.
- Widget mix: route, speed, cadence, heartrate, temperature, gradient, multiple text widgets used as labels and one text widget used as a unit callout below the hero metric.
- Visual treatment: icons only on secondary metrics, no shadows, no elevation, gradient uses a visible positive/negative indicator, route line kept thin to avoid competing with the typography.
- Distinguishing idea: this theme proves that mixed fonts can work if the values remain dominant and the label font is used sparingly.

### 8. Champagne Atlas

- Palette: champagne `#FFF2C9` for values, rust/clay `#801F06` for accents, markers, and emphasis.
- Typography: Furore.
- Composition: premium travel-documentary layout; title top-left, route map upper-right, elevation full width at bottom, two primary metrics in the lower-left quadrant.
- Widget mix: route, elevation, speed, power, time, temperature, title text.
- Visual treatment: icons on, gradient absent, no border, no shadow, route completed in rust and remaining in champagne at lower opacity, elevation area on with champagne fill and rust line, metric elevation label only.
- Distinguishing idea: warm and premium without feeling nostalgic or washed out.

### 9. Champagne Roadbook

- Palette: champagne values, rust labels, cream secondary text.
- Typography: Bebas Neue.
- Composition: roadbook-inspired stacked layout with strong horizontal rhythm; mostly left aligned.
- Widget mix: elevation, speed, cadence, heartrate, time, text widgets used as labels and small unit callouts.
- Visual treatment: no route, elevation area off, no icons, no shadows, both text labels and value spacing should do most of the visual work.
- Distinguishing idea: this is the warm typography-first theme, less map-centric and more story-card oriented.

### 10. Champagne Mechanical

- Palette: champagne base with rust/clay accents and a slightly darker brown for remaining path.
- Typography: Teko.
- Composition: tighter dashboard composition with one hero metric and a compact supporting cluster.
- Widget mix: route, speed, power, temperature, gradient, cadence.
- Visual treatment: no elevation, icons on, route marker deliberately larger, line thickness heavier than other champagne themes, built-in unit color uses rust, no shadows, optional 1px border only on text-heavy widgets.
- Distinguishing idea: the warm palette paired with more aggressive line work makes this feel mechanical rather than elegant.

### 11. Cyan Light-Blue HUD

- Palette: cyan `#00E5FF` with light blue `#7DF9FF`.
- Typography: Saira Stencil.
- Composition: balanced HUD layout with widgets framing the center rather than filling it; route on the right, elevation at the bottom, hero metric upper-left.
- Widget mix: route, elevation, speed, heartrate, cadence, gradient, time.
- Visual treatment: shadows on and used intentionally as glow, icons off, elevation area on with low-opacity cyan fill, route completed and remaining clearly separated by both color and opacity, gradient triangle visible and prominent.
- Distinguishing idea: this is the clearest futuristic option and should look intentionally synthetic, not generic neon.

### 12. Burnt Orange Sprint

- Palette: burnt orange `#FF6A2B` with cream-orange `#DCB9A6` as a minor accent.
- Typography: Bebas Neue.
- Composition: fast lower-left cluster with a large map dominating the right side.
- Widget mix: route, speed, power, heartrate, time.
- Visual treatment: no elevation, no gradient, icons on, no borders, no shadows, thicker route lines, larger route marker, cream units and labels, compact metric count to keep it punchy.
- Distinguishing idea: this is the quick, athletic, sprint-oriented route theme.

### 13. Acid Lime Instrument Lab

- Palette: neon acid lime `#D7FF3F` with titanium gray `#C6CDD5`.
- Typography: JetBrains Mono.
- Composition: tight instrument panel with many small widgets and a prominent bottom elevation strip.
- Widget mix: elevation, gradient, g-force, vertical speed, stride length, air pressure, temperature, text widgets used as labels.
- Visual treatment: route absent, elevation area on, no icons, no shadows, both metric and imperial elevation labels on, decimals used on the scientific-looking metrics, gradient is central rather than secondary.
- Distinguishing idea: this theme should feel like a lab instrument readout, using unusual metrics to justify the palette.

### 14. Dark Navy Nautical

- Palette: dark navy `#043764` with neon teal `#11E9C5`.
- Typography: Furore.
- Composition: open, calm layout with more horizontal spacing than the other saturated themes.
- Widget mix: route, elevation, pace, speed, temperature, time.
- Visual treatment: icons on, no border, very light shadow, elevation area off, route rotated for composition, thin lines, smaller markers, built-in units in neon teal, metric elevation label only.
- Distinguishing idea: this should feel navigational and precise rather than aggressive.

### 15. Lavender Signal

- Palette: lavender `#CAB2FB` with deep violet `#8B5CF6` used as the hard accent.
- Typography: Teko.
- Composition: social-friendly composition with strong top title, balanced center, and a mini elevation band instead of a dominant plot.
- Widget mix: elevation, gradient, cadence, heartrate, core temperature, time, title text.
- Visual treatment: route absent, elevation area on at low opacity, icons mixed on and off, no shadows, gradient gets a visibly colored positive/negative indicator, units stay lighter than the values.
- Distinguishing idea: softer color story, but still clean enough not to become decorative noise.

### 16. Graphite Motorsport

- Palette: graphite black `#1B1B1B`, signal red `#FF3B30`, and off-white for primary values.
- Typography: Inter Black.
- Composition: aggressive motorsport lower band with one large hero speed metric, compact secondary metrics, and a large route map pushing into the right side.
- Widget mix: route, speed, power, g-force, heartrate, gear position, time, temperature.
- Visual treatment: no elevation, no gradient, icons on for the smaller support metrics, no shadows, 1px red border on key text, completed route in signal red and remaining path in off-white at low opacity, thicker line work and larger marker than average.
- Distinguishing idea: this is the missing 16th theme and fills a gap the draft did not cover: a hard-edged high-contrast motorsport treatment that is neither neutral-white nor neon-HUD.

## Coverage check

- Route absent: 2, 6, 9, 13, 15
- Elevation absent: 3, 5, 7, 10, 12, 16
- Elevation area off: 2, 4, 9, 14
- Elevation area on: 1, 6, 8, 11, 13, 15
- Gradient present: 1, 2, 6, 7, 10, 11, 13, 15
- Icons off: 2, 4, 5, 6, 9, 11, 13
- Borders used: 2, 16
- Shadows used: 3, 11, 14
- Elevation labels none: 4
- Elevation labels metric only: 1, 6, 8, 14
- Elevation labels both: 2, 13
- Text widgets used as labels or unit substitutes: 2, 6, 7, 9, 13
- Mixed-font theme: 7
