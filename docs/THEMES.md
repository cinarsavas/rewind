# Themes

Rewind ships with one built-in theme (**Mint**). Everything else is optional: download themes from **Add-ons**, or write your own. A theme is a small TOML file with eleven colours. There are no images, fonts or layout settings to worry about.

## Quick start

1. Open **Add-ons → Themes**.
2. Press **Create example theme**. Rewind writes a commented copy of the default theme into the themes folder.
3. Edit the file, then press **Apply** (or reopen the Add-ons page).
4. The new theme appears in the list with its swatch. Press **Use** to try it.

You can also copy [`docs/theme-template.toml`](theme-template.toml) by hand, or use one of the bundled themes in [`themes/`](../themes) as a starting point. `themes/paper.toml` is a complete light theme.

The themes folder lives next to `config.toml`:

- Windows: `%APPDATA%\rewind\Rewind\config\packs\themes\`
- The **Open themes folder** button in Add-ons takes you straight there.

## File name = theme id

A theme file must be named with lower-case ASCII letters, digits and dashes, up to 40 characters:

```text
sunset.toml          ✓
paper-2.toml         ✓
My Theme.toml        ✗ (spaces and capitals)
theme_final.toml     ✗ (underscore)
```

The file name without `.toml` is the id Rewind stores in `config.toml` when you select the theme. Renaming the file later resets the selection to Mint.

## The file

```toml
name = "Sunset"
author = "you"          # optional, but nice in the Add-ons list

[colors]
nav      = "#1a1016"    # side bar
panel    = "#21151c"    # main page background
card     = "#2b1c24"    # cards, clip rows, dialogs
card_hi  = "#382530"    # raised buttons, hovered rows, inputs
stroke   = "#4a303d"    # thin borders
text     = "#f6e9ee"    # primary text
muted    = "#c2a3b1"    # hints, timestamps, descriptions
faint    = "#8a6d7a"    # disabled labels, quiet captions
accent   = "#ff9e64"    # highlights, primary buttons, replay ring
danger   = "#ff5d6c"    # errors, delete actions
warn     = "#f2b84b"    # warnings
```

`name` is required. `author` is optional. All eleven colours are required.

Colours may be written as `#rrggbb` or `#rgb`. They must start with `#`.

## What each colour controls

| Key | Used for |
| --- | --- |
| `nav` | Side bar, apply bar, splash screens |
| `panel` | Main page background behind cards |
| `card` | Cards, clip rows, dialogs, thumbnail placeholders |
| `card_hi` | Raised buttons, toggle tracks, hovered rows, text inputs |
| `stroke` | Borders around cards, rows and controls |
| `text` | Primary text, headings, values |
| `muted` | Secondary text: hints, file sizes, "5 min ago" |
| `faint` | Tertiary text and disabled controls |
| `accent` | Primary buttons, active nav, replay ring, selection, links |
| `danger` | Errors, delete buttons, destructive dialogs |
| `warn` | Warning cards and hotkey problems |

## Colours Rewind derives for you

You don't set these; they are calculated from the eleven base colours so every theme stays coherent:

| Derived colour | Formula |
| --- | --- |
| `accent_dim` | `mix(card, accent, 0.32)` — selected segmented pill, active toggle track |
| `on_accent` | dark ink `#051410` or white, whichever contrasts more with `accent` |
| `deep` | `panel` darkened by 7% on light themes, 35% on dark themes — chips and slider tracks |
| `accent_tint` | `mix(nav, accent, 0.14)` — selected nav row |
| `hero_on` | `mix(card, accent, 0.07)` — the replay hero card while recording |
| `danger_bg` | `mix(card, danger, 0.18)` |
| `accent_bg` | `mix(card, accent, 0.14)` — info cards and success toasts |
| `warn_bg` | `mix(card, warn, 0.14)` |
| `hover` | `mix(card_hi, text, 0.07)` — button hover fill |

Because these are derived, a good `card`/`text`/`accent` trio does most of the work.

## Readability is enforced

A theme is rejected if it would be hard to read:

- `text` on `card` needs at least **4.5:1** contrast (WCAG AA for normal text).
- `muted` on `card` needs at least **3:1**.
- `muted` on the derived `deep` colour needs at least **3:1**.
- `accent` on `card` needs at least **2.5:1**, otherwise buttons and the ring vanish.

These are the same rules the built-in theme and the bundled themes pass. If a theme fails, Rewind ignores it rather than showing an unreadable interface. Downloaded themes report the exact reason; a hand-dropped file simply won't appear in the list. Check the TOML syntax and contrast first.

## Light themes

Light themes work the same way. Set `panel` and `nav` light, `card` white or near-white, `text` dark, and choose a darker `accent` so it still stands out:

```toml
nav      = "#e8eaf0"
panel    = "#f4f5f9"
card     = "#ffffff"
card_hi  = "#edeff5"
stroke   = "#d3d8e3"
text     = "#1b1f2a"
muted    = "#566074"
faint    = "#8a92a5"
accent   = "#0e9a73"
danger   = "#d63a4a"
warn     = "#a96d00"
```

Rewind automatically keeps inset areas (`deep`) light on a light theme, so chips and slider tracks don't turn into black holes.

## Tips

- Start from a theme that already looks right and change one colour at a time.
- `accent` is the personality colour. Try it first; `card`, `panel` and `nav` can usually stay near the defaults.
- Keep `text` clearly lighter than `card` on dark themes (or darker on light themes). That single pair decides whether the theme is accepted.
- `danger` and `warn` are only used for errors and warnings. They don't need to match the accent.
- Themes only change colours. Animations, smooth scrolling, fonts and layout are separate settings.

## Share a theme

Put the `.toml` file in the themes folder and tell people to drop it there, or submit it to the project so it can be bundled. Every file in `themes/` is validated by the test suite, which keeps a broken theme out of releases.
