# Plan Template: Creating an Asset Directory

Rules Claude must follow when generating a `present` asset directory (the
`assets/<nn>-<topic-slug>/<nn>/*.md` tree consumed by the `present` Rust
TUI at `/Users/esumerfd/GoogleDrive/edward/Personal/projects/present`).

This is a content-authoring template, not a code plan — it governs what
goes *in* the five supported asset kinds (`text.md`, `prompt.md`,
`diagram.md`, `word-cloud.md`, `image.{jpg,jpeg,png}` — see §4 for the
full reference), not the app itself.

---

## 1. Structure: Topic / Panel numbering

```
assets/
  01-topic-name/        # Topics load in numeric order
    01/                 # Panels load in numeric order within a topic
      text.md
    02/
      prompt.md
    03/
      diagram.md
    04/
      word-cloud.md
    05/
      image.jpg          # or image.jpeg / image.png
    06/
      text.md             # multiple asset kinds may share a panel
      prompt.md
  02-another-topic/
    ...
```

- **Topic dirs**: `NN-slug`, zero-padded two digits, kebab-case slug.
  The slug becomes the big-text header rendered at the top of every
  panel in that topic — keep it to 1-3 words so it fits `BigText` at
  `PixelSize::Sextant` without wrapping.
- **Panel dirs**: `NN` only (no slug), zero-padded two digits, numbered
  within their topic. Sequence is the presentation order — never rely
  on filesystem/alphabetical order to "just work"; the number *is* the
  contract.
- **Never skip or reuse numbers** within a topic once a plan is
  finalized — presenters navigate by muscle memory (`l`/`h` through
  panels), and gaps or renumbering mid-deck breaks that.
- One idea per panel. If a panel needs a second idea, it needs a new
  panel number, not more content crammed into one.
- A panel may mix asset types (e.g. `text.md` + `prompt.md`), but each
  asset file still obeys the one-idea rule for its own content.

## 2. Visibility: font size in a terminal

`present` renders in a terminal, so "font size" is fixed by the
presenter's terminal emulator except in two places the app controls
directly — everything else must be sized indirectly, by **limiting how
much text occupies a panel**, since more text always renders smaller
relative to the viewer's fixed viewing distance:

- **Topic labels** render as `BigText` (multi-cell block glyphs) — this
  is the only true "large font" surface. Reserve it for the topic
  name; don't try to replicate it inside `text.md`.
- **Body content** (`text.md`, `prompt.md`) renders as normal terminal
  text at whatever size the presenter's terminal is set to. Research
  on physical displays gives a useful proxy even though there's no
  literal font-size knob here: legible text needs roughly 1 unit of
  letter height per 200 units of viewing distance, and presentation
  rooms commonly run body text at 24pt+ — the practical translation
  for a fixed-size terminal is **fewer, shorter lines**, since that is
  the one lever available to keep effective on-screen size high.
- Concretely: **keep each panel to what fits in one screenful without
  scrolling** — target well under 10 lines of body text. If content
  doesn't fit that comfortably, split it into another numbered panel
  rather than shrinking the perceived size by cramming.
- `word-cloud.md` and single-image panels are inherently
  high-visibility (sparse, large relative to the panel) — prefer them
  over dense text when the content is a set of keywords/concepts
  rather than a full sentence.

## 3. Word count: presenter fills the gaps

Panels are cues for the presenter, not a script to be read aloud.
Apply an assertion-evidence-style limit, adapted for a terminal
audience with no bullet-heavy slides expected:

- **`text.md`**: one heading (the panel label) + 1-3 short sentences
  or a handful of terse bullets. Treat ~40-50 words as the practical
  ceiling for a single panel — if you're writing more, you're writing
  the talk track, not the cue. Compare against real examples already
  in this repo family, e.g.
  `wk-cincy-deliver/assets/01-story-timeline/01/text.md` (one heading,
  one sentence) — that's the target density, not the floor.
- **Bullets, if used**: no more than ~6 per panel, ~6 words per
  bullet (the classic 6x6 guideline) — and treat 6 as a ceiling to
  trim toward, not a quota to fill.
- **`prompt.md`**: the label line is the UI-visible cue; the prompt
  body can be longer since it's sent to Claude rather than read by the
  audience, but each line should still be a single self-contained
  thought — the presenter selects individual lines (`c`/`s`) to send,
  so a line that mixes two ideas can't be sent independently.
- **`word-cloud.md`**: no hard cap, but remember every word gets equal
  visual weight — long lists dilute rather than emphasize. Prefer
  10-20 words/phrases over exhaustive lists.
- **`diagram.md`**: label nodes with words/short phrases, not
  sentences — Mermaid node text wraps poorly in a terminal-rendered
  diagram.

## 4. Choosing an asset type per panel

`present` recognizes exactly five asset kinds (`AssetKind` in
`assets.rs`), matched by exact filename within a panel dir. Anything
else on disk is ignored by the loader.

| File(s)                              | Content is...                          | Notes |
|---------------------------------------|-----------------------------------------|-------|
| `text.md`                             | A single point/quote to land visually   | First `#` heading → panel label; rest is rendered markdown body. |
| `prompt.md`                           | Something to fire at Claude live        | First `#` heading → UI label; rest is the prompt body sent to Claude, line-by-line selectable. |
| `diagram.md`                          | A structure, flow, or relationship      | Raw Mermaid source (no heading extraction) — rendered as a diagram. |
| `word-cloud.md`                       | A set of keywords/concepts to riff on   | Optional `#` heading → cloud title; every other non-blank line is one word/phrase. |
| `image.jpg` / `image.jpeg` / `image.png` | A screenshot/photo/reference          | Checked in that order — only **one** image per panel; first match wins, others are ignored. Auto-resized to fit (max 600x400). |

A panel dir may contain **multiple** of these files at once (e.g.
`text.md` + `prompt.md` together) — `present`'s layout logic arranges
them side-by-side/stacked based on which combination is present. It
may not contain two of the same kind (e.g. two images) — only the
first-matched file per kind loads.

Default to the sparsest type that carries the idea. If torn between
`text.md` and `word-cloud.md` for a list of terms, prefer
`word-cloud.md` — it enforces brevity by construction.

## 5. Checklist before finalizing an asset directory

- [ ] Topic dirs are `NN-slug`, sequential, slug ≤ 3 words
- [ ] Panel dirs are `NN`, sequential, no gaps
- [ ] Every panel expresses exactly one idea
- [ ] Every `text.md` fits on one screen (~<10 lines, ~<50 words)
- [ ] Bullet lists, if any, respect 6 bullets x 6 words as a ceiling
- [ ] `prompt.md` lines are each independently sendable (one thought
      per line)
- [ ] Word clouds stay in the 10-20 word range unless there's a
      specific reason to go wider
- [ ] Each panel dir uses only recognized filenames (`text.md`,
      `prompt.md`, `diagram.md`, `word-cloud.md`,
      `image.jpg`/`.jpeg`/`.png`) and at most one image file
- [ ] Nothing on a panel requires the presenter to read verbatim —
      it's a cue, the talk fills the gaps

---

## Research notes (sources)

- **6x6 rule** — ≤6 bullets/slide, ≤6 words/bullet, as a guideline to
  cut toward rather than a hard quota. [The 6 by 6 Rule for Presentations Explained](https://www.presentationtraininginstitute.com/the-6-by-6-rule-for-presentations-explained/), [Debunking The Presentation 6x6 Rule](https://www.forbes.com/sites/propointgraphics/2017/07/05/debunking-the-presentation-6x6-rule/)
- **Assertion-evidence structure** — one assertion (headline) per
  slide, supported by minimal text/visual evidence rather than bullet
  lists; audiences read at roughly 20 words/minute of on-slide text,
  which is the underlying argument for staying terse.
  [Assertion-Evidence Approach](https://www.assertion-evidence.com/), [Assertion-Evidence Slide Structure (PSU)](http://www.writing.engr.psu.edu/AE_checklist.pdf)
- **Font size / viewing distance** — ~1 unit of letter height per 200
  units of viewing distance; conference-room body text commonly sits
  at 24pt+; back-row-over-8x-screen-height triggers a size bump. Used
  here as the rationale for "fewer/shorter lines" as the terminal
  equivalent of "bigger font."
  [What Font Size Is Best for Presentations?](https://www.presentations.ai/blog/what-font-size-is-best-for-presentations), [Best Font Sizes for Presentations](https://www.whitepage.studio/blog/presentation-font-sizes)

## Related files

- Asset format reference: `present`'s own `README.md`
  (`/Users/esumerfd/GoogleDrive/edward/Personal/projects/present/README.md`)
  — authoritative on file syntax (`text.md`/`prompt.md`/`diagram.md`
  headings, etc.); this template governs *content quality*, not
  syntax.
- App-level feature plans in this dir (`plan-multi-select.md`,
  `design-word-cloud.md`) are unrelated — those plan changes to the
  Rust app itself, not presentation content.
