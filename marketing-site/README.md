# JAPS1ME marketing site

Static promo site for JAPS1ME. Lives in `marketing-site/`, deploys as plain
static files.

## Stack

- **HTML** — single `index.html`, semantic sections, no templating.
- **CSS** — single `styles.css`, vanilla CSS only (custom properties, grid,
  two media queries). No frameworks, no preprocessor, no build step.
- **Alpine.js 3** via CDN (`cdn.jsdelivr.net`, `defer` + `x-data`/`x-show`
  attributes only). No other JavaScript.
- **Fonts** — system sans stack for body, system monospace stack for code.
  No webfonts, works offline except the Alpine CDN.

## Files

- `index.html` — nav, hero (headline, CTAs, terminal, per-game card
  example), format strip, features grid, tabbed commands, safety, Getting
  Started (`#install`), FAQ accordion, footer with Sony non-affiliation
  disclaimer.
- `styles.css` — variables, base, nav, buttons, hero/terminal, XP-style
  glossy `term-bar` + square window buttons, cards, tabs, steps (CSS
  counters), FAQ, footer, `860px` + `480px` breakpoints,
  `prefers-reduced-motion` handling.
- `README.md` — this file.
- `JAPS1ME-v0.2.1-Linux-x86_64.tar.gz` — prebuilt v0.2.1 Linux x86_64 binary
  served as a relative `./` download by the `#download` section (697853 B;
  SHA-256 `a59f866237571f1a464f839d21e51236801926c0619864798baca8dd68aae68c`).
  The older v0.2.0 Windows zip remains in this directory but is not linked;
  add a Windows download card when a v0.2.1 build is available. Re-run
  `sha256sum` after replacing the Linux archive and update the page + this list.

## Favicon

No favicon file: `index.html` draws its own via an inline SVG `data:` URI
in the `<link rel="icon">` tag — a hollow sharp-cornered square with a
periwinkle (`#767ee1`) outline and transparent center, matching the
`.brand-mark`. Kept untilted so it stays crisp at 16×16.

## Palette

- Background pitch black `#000`; surfaces `#0a0a0b` / `#111114` / `#050506`;
  borders `#1f1f23`; text `#f5f5f4`; muted `#9299a9`.
- PlayStation logo gradient (wordmark, hero title, thin divider bars only):
  `linear-gradient(90deg, #f3c300, #00ab9f, #2e6db4, #df0024)`.
- Button/accent palette: aqua `#6be4df` (links, code), periwinkle `#767ee1`
  (secondary buttons, □ brand-mark), coral `#f75f66` (warnings),
  pink `#f69dc8` (minor accents), plus `--gradient-calm`
  (teal→blue, currently unused).
- Accessibility rule: white text always sits on `var(--border)` (`#1f1f23`),
  never on a gradient — applies to `.btn-primary`, `.version-badge`, and
  active tabs.

## Brand-mark

Hollow PS1 □ homage in the nav and footer: `20px` box, `3px solid
var(--periwinkle)`, sharp corners, faint glow, flat `rotate(-12deg)`.
The header copy is deliberately flat (no `rotateX/Y`, no `perspective`):
3D tilts inside the sticky + `backdrop-filter` header warped while
scrolling. The footer keeps a static 3D tilt since it never sticks.

## Interactivity (Alpine only)

- Mobile nav: `x-data="{ open: false }"` toggle.
- Commands: `x-data="{ tab: 'convert', copied: '' }"` tab switcher.
- FAQ: `x-data="{ open: null }"` accordion.
- Copy buttons: `navigator.clipboard.writeText(...)` with transient
  `Copied!` labels. No build-time JS.

## Responsive

- `860px`: hero/cards/footer stack, hamburger nav, 2-col format strip.
- `480px` (iPhone 11 / ~390px): tighter section padding, terminal and
  command rows stack with full-width copy buttons, smaller hero `pre`,
  full-width hero CTAs.
- Long commands/URLs wrap via `overflow-wrap: anywhere` (global on
  `code`/`pre`) plus `min-width: 0` on grid children, so cards never eat
  the right gutter.

## Preview

```sh
python3 -m http.server -d marketing-site
```

Then open the printed `http://localhost:8000` URL. Check `390×844` for
the small-screen pass: no horizontal scroll, hamburger works, tabs wrap.

## Editing conventions

- Keep it two files: structure in `index.html`, all styling in
  `styles.css`.
- Section anchors: `#features`, `#commands`, `#safety`, `#install`
  (labelled "Getting Started" in nav + hero CTA), `#faq`.
- New command tab: add a button + matching `x-show="tab === '…'"` panel
  with a `.cmd` copy row.
- New FAQ item: copy a `.faq-item`, use the next `open === N` index.
- Version bumps: update the hero version badge, Getting Started
  `--version` expectation, and footer line together.
