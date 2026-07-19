# Present

A presentation app that loads structured asset directories as topics and panels, rendered in the terminal.

![Sample slide](assets/sample-slide.png)

## Install

Requires Rust. Install via [rustup](https://rustup.rs):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Build the app:

```bash
make build
```

## Running a Presentation

Pass the path to your assets directory:

Build and install the application:

```bash
make install
```

Then run it with the included demo assets:
```bash
present assets/demo
```

### Presenter notes on a second monitor

Run a second, small window showing the current panel's speaker notes —
open it in a second terminal window and drag that window to your other
display:

```bash
present --notes assets/demo
```

It mirrors whatever topic/panel is live in the main window (updating
within a fraction of a second) and shows that panel's `notes.md`
content. Press `q` in the notes window to quit it independently of the
main presentation.

## Asset Directory Structure

```
assets/
  01-topic-name/        # Topics load in numeric order
    01/                 # Panels load in numeric order within a topic
      text.md
    02/
      prompt.md
    03/
      diagram.md
  02-another-topic/
    ...
```

Each panel directory (`01`, `02`, …) may contain one or more asset files. A panel can mix types — e.g. a `text.md` and a `prompt.md` together.

## Asset Types

### `text.md` — Display content

Markdown rendered in the panel. Use for talking points, explanations, or any content you want visible on screen.

```markdown
# Section Title

Key point one.
Key point two.
```

The first heading becomes the panel label.

### `prompt.md` — Claude prompt

A prompt ready to fire at Claude during the presentation. Displayed in the lower portion of the screen. Move the cursor with `j`/`k` and toggle individual lines into a selection with `c` (each gets a green ✓ highlight). Press `s` to stage — if you've selected lines it sends all of them joined together, otherwise it sends just the line under the cursor. Staging starts a countdown, giving you time to switch to Claude, then the content is automatically typed into an iTerm2 session via AppleScript. If iTerm2 is not available it falls back to copying to the clipboard for manual paste (`⌘V`).

The first heading becomes the label shown in the UI; everything after it is the prompt text sent to Claude.

```markdown
# Prompt label shown in UI

Actual prompt text sent to Claude.
Ask it something specific here.
```

### `diagram.md` — Mermaid diagram

A Mermaid diagram rendered visually in the panel.

```markdown
graph TD
    A[Start] --> B[Process]
    B --> C[End]
```

### `word-cloud.md` — Word cloud

A scattered word cloud rendered in the panel. When paired with a `text.md` in the same panel, the text is shown on top and the word cloud below.

```markdown
---
size: large
---
# Rust Concepts

ownership
borrowing
lifetimes
```

The first heading becomes the cloud's title; each remaining non-blank line becomes one word. The optional `---`-delimited front matter accepts a `size` of `small`, `medium` (default), or `large`.

### `notes.md` — Presenter notes

Speaker notes for this panel, shown only in the `--notes` second-monitor window (see [Presenter notes on a second monitor](#presenter-notes-on-a-second-monitor)) — never on the main audience-facing screen.

```markdown
Slow down here. Make eye contact before advancing.
```

## Navigation

| Key | Action |
|-----|--------|
| `l` / `Space` / `↓` | Next panel |
| `h` / `↑` | Previous panel |
| `→` / `←` | Next / previous topic |
| `j` / `k` | Move prompt cursor down / up |
| `c` | Toggle-select the prompt line under the cursor |
| `s` | Stage send — selected lines if any, else the cursor line — starts countdown, then auto-types into iTerm2 or copies to clipboard |
| `S` | Stage send — all lines in the prompt |
| `C` | Copy the panel's text file path to the clipboard |
| `V` | Open the panel's markdown files in `nvim`, as horizontal splits (`text.md` on top, the rest alphabetically below), cwd'd into the assets dir |
| `R` | Reset to the start |
| `?` | Toggle help |
| `q` | Quit |
