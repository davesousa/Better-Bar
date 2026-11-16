> Written for the 2026-11-16 widget audit (branch widget-polish): the design lead's spec that every widget family follows.

# Laminar Widgets: Design System v1 (Design Lead spec)

Sheets reviewed (dark and light, scale=2, every category plus an `empty` sheet) are in `C:/Users/davej/AppData/Local/Temp/claude/a80c2680d892e8f5/cb900789-feff-4753-aaa4-c3a0fe21de11/scratchpad/audit/design/`. Weather, Clocks and Revenue Goal are the closest to the target. The most common faults are listed in §9.

## 1. Principles

1. **One idea per widget.** Every widget has one key value or one live visual. Everything else supports it.
2. **The key value is never truncated.** Shrink the text with `fit()`, or switch it to compact form ("$5.3K", "1.9M"). Labels may truncate, values may not.
3. **Content first, chrome last.** Show no title where the badge plus a caption already say it. Name each service once per card (badge or title, never both plus a caption).
4. **Progressive disclosure.** Each size larger keeps everything the smaller size shows, in the same reading order, and adds exactly one more layer. Large is Medium, pixel for pixel, plus a detail region.
5. **Calm colour.** Text is neutral. Colour carries meaning (up/down, on/off, the service's identity, a live state) and never decorates.
6. **Optical precision.** Margins are measured to the ink (cap top, baseline, glyph edge), not to text boxes. Corners are concentric.
7. **Windows-native type.** Use Segoe UI Variable with its optical sizes (Small/Text/Display) and Segoe Fluent Icons. Never imitate SF Pro or Apple artwork. Service badges stay original letter or glyph tiles.

## 2. Tokens (logical px; go in `widgets/style.rs`)

### 2.1 Shapes, margins, radii

| Shape | Size | Card radius | Outer margin | Inner tile inset → radius |
|---|---|---|---|---|
| Mini (dock) | 54×54 | 12 | 7 | 6 → 6 |
| Dock M / Dock L | 54 × 140 / 224 | 12 | 12 | 6 → 6 |
| Small | 128×120 | 18 | 14 | 6 → 12, 8 → 10 |
| Medium | 268×96 | 18 | 16 | 6 → 12, 8 → 10 |
| Large | 268×204 | 18 | 16 | 6 → 12, 8 → 10 |

- **Concentric rule:** any panel whose corner sits near the card corner gets radius = card radius − inset, never less than 6. This covers toggle tiles, album art and full-bleed charts. Free-floating elements use their own token.
- **Badge radius** = 0.25 × badge size. Buttons and pills are fully round.
- **Spacing scale:** 2, 4, 6, 8, 10, 12, 16, 20, 24. No other gaps.
- **Card edge:** 1 px inner stroke. Dark: white @ 0.08. Light: black @ 0.06.
- **Card fill:** dark white @ 0.07. Light white @ 0.62.

### 2.2 Type scale

Font choice is automatic from size (see §8.3). The weight given here is what to pass.

| Role | Size | Weight | Colour | Use |
|---|---|---|---|---|
| Hero XL | 44 | 600 (time/temp 300) | text1 | Large-only heroes, clocks |
| Hero | 32 | 600 (time/temp 300) | text1 | Medium/Large key value; `fit` min 22 |
| Hero S | 26 | 600 (time/temp 400) | text1 | Small key value; `fit` min 18 |
| Value | 17 | 600 | text1 | Dock value, chart-header value, secondary stat values |
| Title | 13 | 600 | text1 | Header-row title, list emphasis |
| Body | 13 | 400 | text1 | List row labels |
| Caption | 12 | 400 | text2 | Line under a hero; Small caption |
| Footnote | 11 | 400 / 500 | text2 (meta text3) | Trailing row values, dock caption, meter labels |
| Micro | 10 | 600 | text3 or accent | Axis ticks, weekday letters, "PM", mode labels ("FOCUS"); ≤ 6 chars may be uppercase |

- Nothing below 10 px, except Mini values, which may fit down to 11.
- "AM/PM" suffix: Micro 10/600 text2, top-aligned to the hero's cap top, 3 px after the digits.
- Mini value: 14/600, fit to 11.
- **Numerals:** tabular figures everywhere (set globally, §8.3).
- **Number formats:**
  - Money: cents only when under $100. $100–9,999 → "$5,336". ≥ $10,000 → "$12.4K" / "$1.2M".
  - Tokens: "1.9M", "412K".
  - Percent: one decimal under 10 ("8.4%"), whole numbers otherwise.
  - Durations: "6h 26m".
  - Change: "+12.4%" / "−3.1%" (U+2212 minus), no triangles.
- **Compact budgets:** Mini ≤ 5 glyphs ("$5.3K", "82%", "5:17", "12"). Dock value ≤ 8 glyphs.

### 2.3 Colour roles

| Token | Dark | Light |
|---|---|---|
| text1 | #FFFFFF @ 0.95 | #1C1C1E @ 1.0 |
| text2 | #EBEBF5 @ 0.64 | #3C3C43 @ 0.72 |
| text3 | #EBEBF5 @ 0.38 | #3C3C43 @ 0.46 |
| track (ring/bar/heat-empty) | white @ 0.12 | black @ 0.08 |
| separator | white @ 0.10 | black @ 0.08 |
| plate (glyph tile, secondary button) | white @ 0.12 | black @ 0.06 |

System accents. Dark uses the iOS dark values already in paint.rs. Light swaps automatically (§8.2):

| Accent | Dark | Light fill | Light text |
|---|---|---|---|
| blue | 0A84FF | 007AFF | 0062CC |
| green | 30D158 | 34C759 | 248A3D |
| red | FF453A | FF3B30 | D70015 |
| orange | FF9F0A | FF9500 | C45500 |
| purple | BF5AF2 | AF52DE | 8944AB |
| yellow | FFD60A | FFCC00 | A05A00 |

- **Semantic up/down:** green/red only where "up" is a judgment: money, prices, followers. A zero or missing change is text2.
- **Identity accent:** each widget's chart, meter and ring use the service colour or one fixed metric colour. CPU = blue, memory = purple, battery = green (yellow ≤ 20%, red ≤ 10%), disk = orange, Claude/AI tokens = Claude orange, analytics = blue.
- An AI-token chart is never green "because it's a chart".
- **On/off controls:** on = blue fill + white glyph. Off = plate + text1 glyph.
- **Primary action button:** the widget's mode colour (Focus red, Timer blue, Stopwatch orange).

### 2.4 Badges, icons, controls

| Element | Mini | Dock | Small | Medium/Large |
|---|---|---|---|---|
| Service badge | 20 (value-led) / 38 (visual-led) | 30 | 28 | 24 top-right (Glance) / 20 inline (Header) |
| Fluent glyph | 18 / 22 | 16 in a 30 plate | 16 in a 28 plate | 16 (header), 24 (Large feature) |
| Primary button | — | 30 | 28 | 36 |
| Secondary button | — | 26 | 24 | 28 |

- Glyph inside a button or plate = 0.45 × its diameter.
- Hit targets ≥ 28.
- Near-black badge backgrounds (Cursor, Codex, Watchlist, GitHub) get a 1 px inner stroke of white @ 0.14 in dark theme, so they don't sink into the glass.

### 2.5 Charts, rings, bars, lists

- **Line/area chart:**
  - Stroke 2.0 (height ≥ 40) or 1.5 (sparklines). Round caps and joins.
  - Fill is a vertical gradient: accent @ 0.30 (dark) / 0.22 (light) at the top, fading to 0 at the bottom.
  - Y-range = data min − 8% … max + 14% of the range, so peaks never touch the box.
  - Smooth with Catmull-Rom when there are ≥ 8 points.
  - End dot: radius 3, accent, with a 1.5 ring of the card colour (Medium/Large only).
  - No axes, grid or labels. One optional baseline hairline (separator colour).
  - The chart box always sits inside the outer margins and never touches a card edge.
- **Ring:**
  - Thickness = max(3, 0.11 × diameter). Track colour; starts at 12 o'clock, clockwise, round caps.
  - Centre number at 0.26 × diameter, 600, text1; only when diameter ≥ 30.
- **Meter bar:** height 4 (Mini/Dock/Small) or 6 (Medium/Large), fully round, track behind it, minimum visible fill = its height.
- **Lists:**
  - Row pitch 24 (Large), 20 (Medium), 18 (Small).
  - Label Body 13 (11 on Small) text1. Trailing value Footnote 11/500 text2, right-aligned.
  - Max rows: Small 3, Medium 2, Large 5.
  - No separators between rows. If fewer rows exist than fit, rows stay top-aligned at their pitch and the region below is not left bare (see Large rules).
- **Heat map:** cell gap 2, cell radius 2 (Medium) / 3 (Large), empty = track, four intensity steps of the accent at 0.35 / 0.55 / 0.75 / 1.0.

## 3. Layout templates (coordinates are cap-top / baseline unless noted)

### Mini (54×54)
- **Value-led:** badge or glyph 20, centred, top at 8. Value 14/600 centred, baseline 45.
- **Visual-led** (ring, dial, disc, toggle, day tile): the visual 38–40, centred. A number inside a ring only if it fits at ≥ 11.
- It must read like an app icon: one mark, at most one value, no captions.

### Dock M (54 × 140)
- Badge 30 at (12, 12). Text column x = 52 to w − 12.
- Value 17/600, baseline 30. Caption 11 text2, baseline 44.
- With a meter: caption baseline 40; bar 3 px tall at y = 45.

### Dock L (54 × 224)
- Dock M plus an accessory slot on the right: width = 34% of the pill, x = w − 12 − slot, y 14 to 40.
- The slot holds one of: a sparkline, a meter with its label, primary controls (timers, media) or a second value.
- **Dock L must never be Dock M stretched.**

### Small (128×120), bottom-anchored
- Badge or visual 28 at (14, 14).
- Top-right accessory slot: x 52 to 114, y 16 to 42. It holds the change figure (11/600, right-aligned, baseline 26), a sparkline (y 30 to 42 under a change figure, or 16 to 42 alone), or a meter (4 px at y 27).
- Hero S baseline 86. Caption 12 text2 baseline 106.

### Medium, two templates only

- **A. Glance** (one value):
  - Hero 32, cap top 16, x 16.
  - Caption 12 text2, baseline 58.
  - Third line (11 text3) baseline 76, or a meter 6 px at y 70.
  - Badge 24 top-right at (w − 40, 16).
  - Right accessory box: x 148 to 252, y 48 to 80 (sparkline, ring or mini-stats).
- **B. Header** (charts, lists, multi-value):
  - Header row: badge 20 at (16, 16), title 13/600 baseline 31 at x 44.
  - Trailing value 17/600 or meta 11 text2, right-aligned at w − 16, baseline 31. A change figure under a trailing value sits at baseline 45.
  - Content region: y 44 to 80 (52 to 80 under a two-line trailing block).

### Large
- **Glance faces:** rows 0 to 96 are pixel-identical to Medium. Separator at y 96 (x 16 to 252). Detail region y 108 to 188.
- **Header faces:** the same header, then content y 44 to 188.
- **Detail fallback order:** history chart (≥ 64 tall) → up to 5 rows → a three-stat footer (value 17/600 + footnote label, three equal columns) → a forecast or timeline strip.
- If a face has none of these, its Large is a designed composition (§5), never Medium plus emptiness.

## 4. Information architecture per size

| Size | Must show | Must not |
|---|---|---|
| Mini | The key value (compact) or the live visual | Captions, titles, two values |
| Dock M | Key value + one-word context | Repeat the service name if the badge says it |
| Dock L | Dock M + one trend, secondary value or primary control | Be identical to Dock M |
| Small | Key value + context + one accessory | Header rows, more than 3 rows |
| Medium | Small + a second dimension (chart, 2 rows, breakdown, controls) | Stretch Small; repeat a caption that equals the title |
| Large | Medium unchanged + a detail region that fills to the bottom margin | Empty bottom half; content touching edges |

- Titles appear only in Header template cards. Captions say what the number is ("Unread", "tokens today", "Last 30 days"), never the service name when the badge already shows it. Example: stripe-chart Small caption must read "Last 30 days", not "Stripe".

## 5. Family notes (non-connector faces)

- **Single-action tools** (dark-mode, focus, notifications, show-desktop, keep-awake, screenshot, run, search, recycle-bin, windows-settings):
  - Small: plate 44 centred at y 22; label 13/600 centred, baseline 88; state 11 text2, baseline 104.
  - Medium: plate 44 at (16, 26), label + state to its right.
  - Large: a state tile at inset 6 (radius 12), full width and 112 tall, with a glyph 32 in a 56 circle (blue when on), label 17/600 and state 13 text2. Below it, two footnote rows of real facts (e.g. "Until 6:00 PM", "3 notifications", "Recycle bin: 214 MB"). Use a one-line description only when no fact exists.
- **Clocks:**
  - Time heroes use weight 300 at ≥ 32, 400 at 20–31 and 600 below 20. Never 200.
  - Seconds only in Large digital, in text3.
  - World-clock labels are short city names ("SF", "London", "Tokyo"), never cut mid-word ("SAN").
  - White analog faces in light theme get a separator-colour hairline.
  - analog-square Small must sit inside a card with concentric radius.
- **Weather:** the reference family. Temperature follows the time-hero weights. Keep emoji icons. Large forecast rows use the 24-row pitch.
- **Timers:** label Micro in the mode colour.
  - Small: hero top-left; reset (secondary 24) and play (primary 28) right-aligned on the bottom margin line, 8 apart.
  - Large ring diameter 120 stays.
  - Dock L puts play in the accessory slot.
- **Media:**
  - Title 13/600 + artist 12 text2 must never truncate to under 10 characters. Shrink the art before truncating the title.
  - Large: art 96 at inset 16 (radius 10), title/artist to its right, controls centred along the bottom margin.
- **System rings** (cpu, memory, battery, disk): Large adds a 60-sample history chart, or for battery/disk a three-stat footer (time left / health / power source; used / free / total).
  - network Large: two-series chart (down blue, up green) with the current rates in the header.
  - brightness with no data: shows the setup state (§7) with "Not supported on this display", not "—".
- **Lists/feeds** (linear, agenda, github-overview, top-cpu, top-memory, recent-files, watchlist): keep the row pitch. Large shows 5 rows. With fewer items, end with a footnote summary row ("6 open · 2 in progress").
  - watchlist Large adds a 48 px sparkline per row between symbol and price, not wider spacing.
- **Apps grid:** tiles 44, radius 10, gap 12. Rows must be full. Large is a 4×3 grid; never leave an orphan tile on its own row.

## 6. Light vs dark

1. Every colour comes from a token or accent. Never hard-code white text except on accent fills (pills, on-toggles, badge glyphs).
2. Accents swap to their light fill variant for fills and their light text variant for text (§8.2).
3. Light charts use fill top @ 0.22. Tracks and plates use black alpha, never grey hex.
4. White content (analog faces, day-page tile, Connect pill text) gets a hairline in light theme. Weather emoji keep the existing silhouette shadow.
5. Brand badge colours stay the same in both themes. Near-black badges get the dark-theme stroke (§2.4).
6. Check every widget in both sheets. A light-theme value under 4.5:1 contrast against white @ 0.62 over a light grey backdrop fails.

## 7. Placeholder, setup, loading, error

- **Loading:** a skeleton of the real layout, with no "Loading…" text.
  - The real badge and the real title (Header template) are drawn.
  - The hero is replaced by a rounded bar (track colour, radius 6) at hero cap height × 60% of the hero box width. The caption becomes a 40×8 bar.
  - Large draws a skeleton baseline plus three 8 px bars in the detail region.
  - Mini and Dock draw only the badge (Dock also gets a 60×10 value bar).
- **Setup** (needs connection): the whole card is the Connect hit area.
  - Mini: badge 30 centred, plus a 7 px blue dot at the top-right of the badge.
  - Dock: badge + name 13/600 + "Connect" in Footnote 11/600, blue text.
  - Small: badge 28 top-left; name 13/600 baseline 66; one-line reason 11 text2 baseline 82; pill full width × 26 at the bottom margin, blue, "Connect" 12/600 white.
  - Medium: Glance layout with badge 28 top-left; name 15/600; reason 12 text2; pill 88×28 right-aligned on the bottom margin.
  - Large: centred column with badge 48 (top at 36), name 15/600 baseline 108, reason 12 text2 baseline 128, pill 132×30 with its top at 150.
- **Error with no data:** setup layout without the pill. The reason line starts with a 12 px warning glyph (E7BA) in orange.
- **Error with stale data:** show the data, plus a 6 px orange dot after the caption.

## 8. Foundation changes (do these before any family work)

### 8.1 New `native/laminar/src/widgets/style.rs`

Named `pub const`s for everything in §2:
- `CARD_R = 18.0`, `DOCK_R = 12.0`
- `M_MINI = 7.0`, `M_DOCK = 12.0`, `M_SMALL = 14.0`, `M_CARD = 16.0`
- `SP: [f32; 9]`
- type roles as a `Type { size, weight }` const per role: `HERO_XL`, `HERO`, `HERO_S`, `VALUE`, `TITLE`, `BODY`, `CAPTION`, `FOOT`, `MICRO`, `MINI_VALUE`
- `BADGE_MINI = 20.0`, `BADGE_MINI_VIS = 38.0`, `BADGE_DOCK = 30.0`, `BADGE_SMALL = 28.0`, `BADGE_GLANCE = 24.0`, `BADGE_HEADER = 20.0`, `BADGE_R = 0.25`
- `BTN_PRIMARY = 36.0`, `BTN_SECONDARY = 28.0`
- `ROW_L = 24.0`, `ROW_M = 20.0`, `ROW_S = 18.0`
- `STROKE = 2.0`, `STROKE_SPARK = 1.5`, `FILL_DARK = 0.30`, `FILL_LIGHT = 0.22`
- `METER_H = 6.0`, `METER_H_COMPACT = 4.0`, `RING_T = 0.11`
- `SEP_Y_LARGE = 96.0`, `DETAIL_TOP = 108.0`, `HEADER_BASE = 31.0`, `CONTENT_TOP = 44.0`

### 8.2 `canvas.rs` theme

- Implement the §2.3 tokens: `text1/2/3`, plus new `track()`, `sep()`, `plate()`, `card()`, `card_edge()`.
- `ring()` and `bar()` use `track()`.
- Add `Canvas::tone(col, for_text: bool)`. In light theme it maps the six dark system hexes to their light fill or light text variant and leaves every other colour unchanged. Call it in `brush()`. `text()` passes `for_text = true` so text gets the text variant. This fixes light-theme accents across all of paint.rs with no per-call churn.
- Add `pub fn green()/red()/blue()/orange()/purple()/yellow()` for clarity in new code.

### 8.3 `canvas.rs` text

- **Automatic optical size in `format()`** (except `Font::Icons`): size < 12 → "Segoe UI Variable Small"; 12–19.99 → "Segoe UI Variable Text"; ≥ 20 → "Segoe UI Variable Display".
- **Tabular figures:** apply `DWRITE_FONT_FEATURE_TAG_TABULAR_FIGURES` to every text layout via `IDWriteTypography` + `SetTypography` over the full range.
- **`Canvas::cap_top(size) -> f32` and `baseline(size) -> f32`:** offsets from the text-box top, computed from the font's design metrics (ascent, capHeight / unitsPerEm), cached. Add `text_base(s, x, baseline, …)`, which draws by baseline. Layouts in this spec use these.

### 8.4 `canvas.rs` charts

- `chart(x, y, w, h, vals, col, ChartOpts { fill, dot, stroke, smooth })` implements §2.5: padding, gradient, Catmull-Rom smoothing and the end dot.
- Keep `area()` as a wrapper that picks the stroke from h (≥ 40 → 2.0, else 1.5) with `smooth = vals.len() >= 8`.

### 8.5 Cards

- `paint.rs bg()`: radius `CARD_R` / `DOCK_R`, then a `card_edge()` 1 px inner stroke.
- `launchpad_widgets.rs:147` hard-codes the shadow radius as 18. Replace it with `style::CARD_R` so card and shadow stay locked together.

### 8.6 Shared helpers in `paint.rs` (family editors must use these)

- `header(c, badge, title, trailing: Option<(&str, Col)>) -> f32`: Header template; returns the content top.
- `glance(c, out, badge, value, caption, third: Option<Line>)`: Medium Glance block; Large reuses it unchanged.
- `separator(c, y)`
- `detail_region(c) -> (x, y, w, h)`
- `rows(c, items, top, pitch, max)`
- `stat_footer(c, [(value, label); 3], top)`
- `skeleton(c, x, y, w, h)`
- `pill_button(c, out, x, y, w, h, label, act)`
- `fmt_money(v, budget)`, `fmt_compact(v)`, `fmt_delta(d) -> (String, Col)`: replaces `delta_str`, using §2.2 formats.
- `fit_value(c, s, role, max_w) -> (String, f32)`: tries the full string, then the compact form, then shrinks to the role minimum. It never ellipsizes.

### 8.7 Shared compact layouts and placeholder

- Rewrite `compact_sum()` to the §3 Mini, Dock M/L (accessory slot) and Small templates.
- Rewrite `placeholder()` to §7.
- Rewrite `details()` to the §3 Large detail region (inside the margins, bottom margin respected).

These three changes alone fix most connector widgets at the compact sizes.

### 8.8 Verification

After the foundation pass, `cargo build -p laminar` must have no warnings and `cargo test -p laminar` no new failures. Render all categories dark and light at scale=2, plus `empty`, and compare against this spec before handing families out.

## 9. Observed defects to target (current build)

- **Large looks empty:** openrouter, cursor, revenuecat, gmail, notes, whatsapp, agenda, linear, github-overview, network, battery, disk, language, uptime, brightness, all single-action tools, apps (orphan tile), ai-usage, resource-monitor.
- **Large charts run into the card's bottom edge:** stripe, stripe-chart, claude-code(-week), anthropic-cost, openai-cost, codex, stock-chart.
- **Mini values overflow or ellipsize:** "$126.00", "$83.46", "3 today", "6h 26m". Dock L is identical to Dock M for most faces.
- **Badge placement and size are inconsistent:** Stat top-right 34, Usage left 36, Chart inline 20. Duplicate naming: stripe-chart caption "Stripe" under an "S" badge.
- **Wrong or meaningless chart colour:** green for AI tokens.
- **Truncation:**
  - revenue-goal Large: "of $10,000 goal · l…"
  - now-playing Large: "Midnight D…"
  - brightness: "(DD…"
- **Light theme:** green/blue accent text has low contrast; text2 is too weak.
- **Placeholders:** they say "Loading…" at every size, and Large is three-quarters empty.

## 10. Audit checklist (tick per widget × Mini, Dock M, Dock L, Small, Medium, Large × dark, light)

1. One key value or visual is obvious in under 1 second.
2. The key value is fully visible (no ellipsis), formatted per §2.2, with tabular digits.
3. Margins match §2.1 to the ink; nothing touches or clips a card edge.
4. Only §2.2 type roles are used; no size below 10 (Mini ≥ 11).
5. Only token or accent colours are used; semantic green/red only where up/down is a judgment.
6. Badge size and position match §2.4 / §3; the service is named once.
7. Each size adds exactly one layer over the previous; Dock L ≠ Dock M; Large is Medium plus a filled detail region.
8. Corners are concentric; inner panels follow radius = card radius − inset.
9. Charts, rings and meters follow §2.5 (stroke, fill, headroom, track, end dot).
10. Light theme: contrast is OK, accents are swapped, white content has a hairline.
11. Empty, loading, setup and error states match §7 at every size.
12. Hit targets ≥ 28; controls are aligned to the margin grid.
13. Nothing is copied from Apple artwork; badges are original letter or glyph tiles.
