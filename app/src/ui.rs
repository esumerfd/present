use crate::app::{App, Screen};
use crate::assets::{AssetKind, Panel, WordCloudSize};
use crate::firework::{Firework, COLORS};
use crate::notes::NotesApp;
use std::collections::HashSet;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use tui_big_text::{BigText, PixelSize};

const CLOUD_COLORS: &[Color] = &[
    Color::Cyan,
    Color::Yellow,
    Color::Green,
    Color::Magenta,
    Color::White,
    Color::Red,
];

fn word_hash(word: &str) -> u64 {
    word.bytes().fold(0xcbf29ce484222325u64, |acc, b| {
        acc.wrapping_mul(0x100000001b3).wrapping_add(b as u64)
    })
}

fn cloud_seed(words: &[String]) -> u64 {
    words.iter().fold(0x517cc1b727220a95u64, |acc, w| {
        acc.wrapping_add(word_hash(w)).rotate_left(7)
    })
}

fn cloud_position(seed: u64, wh: u64, index: usize, width: u64, height: u64) -> (u64, u64) {
    let i = index as u64;
    let x1 = (seed ^ wh ^ i.wrapping_mul(2654435761)) % width;
    let x2 = (seed.wrapping_add(1) ^ wh.rotate_left(17) ^ i.wrapping_mul(1234567891)) % width;
    let x3 = (seed.wrapping_add(2) ^ wh.rotate_right(13) ^ i.wrapping_mul(9876543219)) % width;
    let raw_x = (x1 + x2 + x3) / 3;

    let y1 = (seed ^ wh.rotate_left(32) ^ i.wrapping_mul(6364136223846793005)) % height;
    let y2 = (seed.wrapping_add(3) ^ wh.rotate_left(48) ^ i.wrapping_mul(4294967311)) % height;
    let y3 = (seed.wrapping_add(5) ^ wh.rotate_right(21) ^ i.wrapping_mul(2147483659)) % height;
    let raw_y = (y1 + y2 + y3) / 3;

    (raw_x, raw_y)
}

const MAX_PLACEMENT_ATTEMPTS: u64 = 32;
const WORD_PADDING: u16 = 1;

struct PlacedWord {
    word: String,
    /// Draw origin (inset by `WORD_PADDING`), relative to the inner area's top-left corner.
    x: u16,
    y: u16,
}

/// Places words one at a time, in list order, retrying each word's candidate position
/// (up to `MAX_PLACEMENT_ATTEMPTS` times) until it finds one that doesn't overlap any
/// already-placed word. A word that finds no free spot is dropped rather than rendered
/// on top of another word. Deterministic: the same words + area always place the same way.
///
/// Words are placed biggest-footprint-first (the classic tag-cloud packing heuristic --
/// placing large items while the canvas is still open, then backfilling smaller ones
/// into the remaining gaps, fits more words overall than placing in list order). Each
/// word first tries a handful of cheap pseudo-random candidate positions, which keeps
/// the cloud's organic scattered look in the common, uncrowded case; if all of those
/// collide, it falls back to an exhaustive scan of every remaining position so a word
/// is only ever dropped when there truly is no room left for it, not because random
/// sampling missed a gap that was actually there.
fn place_words(words: &[String], repeat: u16, inner_width: u16, inner_height: u16) -> Vec<PlacedWord> {
    let seed = cloud_seed(words);
    let mut placed_rects: Vec<Rect> = Vec::new();
    let mut result = Vec::new();

    let mut order: Vec<usize> = (0..words.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(words[i].chars().count()));

    for i in order {
        let word = &words[i];
        let wh = word_hash(word);
        let footprint_w = word.chars().count() as u16 * repeat + WORD_PADDING * 2;
        let footprint_h = 1 + WORD_PADDING * 2;
        if footprint_w > inner_width || footprint_h > inner_height {
            continue;
        }

        let x_range = (inner_width - footprint_w + 1) as u64;
        let y_range = (inner_height - footprint_h + 1) as u64;

        let mut chosen = None;
        for attempt in 0..MAX_PLACEMENT_ATTEMPTS {
            let (raw_x, raw_y) = cloud_position(seed, wh.wrapping_add(attempt), i, x_range, y_range);
            let candidate = Rect {
                x: raw_x as u16,
                y: raw_y as u16,
                width: footprint_w,
                height: footprint_h,
            };
            if !placed_rects.iter().any(|r| r.intersects(candidate)) {
                chosen = Some(candidate);
                break;
            }
        }

        let chosen = chosen.or_else(|| find_free_spot(&placed_rects, footprint_w, footprint_h, inner_width, inner_height));

        if let Some(rect) = chosen {
            result.push(PlacedWord {
                word: word.clone(),
                x: rect.x + WORD_PADDING,
                y: rect.y + WORD_PADDING,
            });
            placed_rects.push(rect);
        }
    }

    result
}

/// Exhaustively scans every valid top-left position for a `width` x `height` footprint,
/// row by row, returning the first one that doesn't intersect any already-placed rect.
/// `width`/`height` are assumed to already fit within `inner_width`/`inner_height`.
fn find_free_spot(placed: &[Rect], width: u16, height: u16, inner_width: u16, inner_height: u16) -> Option<Rect> {
    for y in 0..=(inner_height - height) {
        for x in 0..=(inner_width - width) {
            let candidate = Rect { x, y, width, height };
            if !placed.iter().any(|r| r.intersects(candidate)) {
                return Some(candidate);
            }
        }
    }
    None
}



/// An image asset that couldn't be drawn through ratatui's own cell buffer and instead
/// needs a raw terminal-graphics-protocol escape sequence written directly to the real
/// terminal, positioned at (x, y) in terminal cells relative to the frame's origin.
pub struct PendingImage {
    pub x: u16,
    pub y: u16,
    pub escape: String,
}

pub fn render(f: &mut Frame, app: &App) -> Option<PendingImage> {
    let iterm2_supported = crate::iterm2::detect_support();
    match app.screen {
        Screen::Intro => {
            render_intro(f, app);
            None
        }
        Screen::Topic => render_topic(f, app, iterm2_supported),
        Screen::Help => {
            let pending = render_topic(f, app, iterm2_supported);
            render_help(f);
            pending
        }
        Screen::Confirm => {
            let pending = render_topic(f, app, iterm2_supported);
            render_confirm(f, app);
            pending
        }
        Screen::Countdown => {
            let pending = render_topic(f, app, iterm2_supported);
            render_countdown(f, app);
            pending
        }
    }
}

fn joke_lines(joke: &str, max_chars: usize) -> Vec<Line<'static>> {
    let max_chars = max_chars.max(10);
    let mut result: Vec<Line<'static>> = Vec::new();
    let parts: Vec<&str> = joke.split(". ").collect();
    for (i, part) in parts.iter().enumerate() {
        let sentence = if i + 1 < parts.len() {
            format!("{}.", part)
        } else {
            part.to_string()
        };
        let mut current = String::new();
        for word in sentence.split_whitespace() {
            if current.is_empty() {
                current.push_str(word);
            } else if current.len() + 1 + word.len() <= max_chars {
                current.push(' ');
                current.push_str(word);
            } else {
                result.push(Line::from(current));
                current = word.to_string();
            }
        }
        if !current.is_empty() {
            result.push(Line::from(current));
        }
    }
    result
}

fn render_intro(f: &mut Frame, app: &App) {
    let area = f.area();
    // Sextant: each character is 4 terminal columns wide, 3 rows tall
    let max_chars = (area.width / 4) as usize;
    let joke = crate::app::JOKES[app.joke_index];
    let joke_lines = joke_lines(joke, max_chars);
    let joke_height = (joke_lines.len() as u16 * 3).max(3);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(16),
            Constraint::Length(joke_height),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

    render_scaled_name(f, "Ed", 2, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD), chunks[1]);

    let big_joke = BigText::builder()
        .pixel_size(PixelSize::Sextant)
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .lines(joke_lines)
        .centered()
        .build();
    f.render_widget(big_joke, chunks[3]);

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "Press SPACE to find out.",
            Style::default().fg(Color::DarkGray),
        )))
        .alignment(Alignment::Center),
        chunks[4],
    );
}

fn render_scaled_name(f: &mut Frame, text: &str, scale: usize, style: Style, area: Rect) {
    use font8x8::UnicodeFonts;
    let mut rows: Vec<String> = vec![String::new(); 8 * scale];
    for ch in text.chars() {
        if let Some(glyph) = font8x8::BASIC_FONTS.get(ch) {
            for (pixel_row, &row_byte) in glyph.iter().enumerate() {
                for pixel_col in 0..8usize {
                    let cell = if (row_byte >> pixel_col) & 1 != 0 { "██" } else { "  " };
                    for sy in 0..scale {
                        rows[pixel_row * scale + sy].push_str(cell);
                    }
                }
            }
        }
        for row in &mut rows {
            row.push_str("  ");
        }
    }
    let lines: Vec<Line> = rows.into_iter().map(|s| Line::from(Span::styled(s, style))).collect();
    f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}

fn render_topic(f: &mut Frame, app: &App, iterm2_supported: bool) -> Option<PendingImage> {
    let area = f.area();
    let topic = app.topics.get(app.current_topic)?;
    let has_prompt = topic.current_panel().map(|p| p.has_prompt()).unwrap_or(false);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(4),
            Constraint::Length(1),
        ])
        .split(area);

    render_header(f, app, chunks[0]);

    let pending = topic.current_panel().and_then(|panel| {
        render_panel(f, panel, chunks[1], app.selected_line, &app.selected_lines, iterm2_supported)
    });

    render_status(f, app, has_prompt, chunks[2]);

    if let Some(fw) = &app.firework {
        render_firework(f, fw, area);
    }

    pending
}

pub fn render_notes(f: &mut Frame, app: &NotesApp) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(4), Constraint::Length(3)])
        .split(area);

    let topic_label = app.current_topic_label().unwrap_or("(no topic)");
    let panel_number = app.current_panel.saturating_add(1);
    let panel_total = app
        .topics
        .get(app.current_topic)
        .map(|t| t.panels.len())
        .unwrap_or(0);
    f.render_widget(
        Paragraph::new(Span::styled(
            format!(" {topic_label} — Panel {panel_number} of {panel_total}"),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ))
        .block(Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(Color::DarkGray))),
        chunks[0],
    );

    let notes_content = app
        .current_panel()
        .and_then(|p| p.notes())
        .and_then(|a| match &a.kind {
            AssetKind::Notes { content } => Some(content.clone()),
            _ => None,
        });
    let body = match notes_content {
        Some(content) => Paragraph::new(crate::markdown::render_to_lines(&content)),
        None => Paragraph::new(Span::styled(
            "(no notes for this panel)",
            Style::default().fg(Color::DarkGray),
        )),
    };
    f.render_widget(
        body.block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)))
            .wrap(Wrap { trim: false }),
        chunks[1],
    );

    f.render_widget(
        Paragraph::new(Span::styled(
            " q: quit",
            Style::default().fg(Color::DarkGray),
        ))
        .block(Block::default().borders(Borders::TOP).border_style(Style::default().fg(Color::DarkGray))),
        chunks[2],
    );
}

fn render_panel(
    f: &mut Frame,
    panel: &Panel,
    area: Rect,
    selected_line: usize,
    selected_lines: &HashSet<usize>,
    iterm2_supported: bool,
) -> Option<PendingImage> {
    let has_text       = panel.assets.iter().any(|a| matches!(a.kind, AssetKind::Text { .. }));
    let has_diagram    = panel.assets.iter().any(|a| matches!(a.kind, AssetKind::Diagram { .. }));
    let has_prompt     = panel.has_prompt();
    let has_word_cloud = panel.has_word_cloud();
    let has_image      = panel.has_image();

    if has_image && !has_text && !has_diagram && !has_word_cloud && !has_prompt {
        return render_image_asset(f, panel, area, iterm2_supported);
    }
    if has_image && has_prompt && !has_text && !has_diagram && !has_word_cloud {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
            .split(area);
        let pending = render_image_asset(f, panel, rows[0], iterm2_supported);
        render_prompt_asset(f, panel, rows[1], selected_line, selected_lines);
        return pending;
    }
    if has_image && has_text && !has_prompt && !has_diagram && !has_word_cloud {
        let sides = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);
        let pending = render_image_asset(f, panel, sides[0], iterm2_supported);
        render_text_asset(f, panel, sides[1]);
        return pending;
    }

    match (has_text, has_diagram, has_prompt, has_word_cloud) {
        // text + diagram takes priority over word cloud when all three present
        (true, true, _, _) => {
            let sides = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            render_text_asset(f, panel, sides[0]);
            render_diagram_asset(f, panel, sides[1]);
        }
        (true, false, true, _) => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
                .split(area);
            render_text_asset(f, panel, rows[0]);
            render_prompt_asset(f, panel, rows[1], selected_line, selected_lines);
        }
        (false, true, true, _) => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
                .split(area);
            render_diagram_asset(f, panel, rows[0]);
            render_prompt_asset(f, panel, rows[1], selected_line, selected_lines);
        }
        (_, true, false, false) => render_diagram_asset(f, panel, area),
        (true, false, false, false) => render_text_asset(f, panel, area),
        // word cloud cases (before the wildcard prompt-only arm)
        (false, false, false, true) => render_word_cloud_asset(f, panel, area),
        (false, false, true, true) => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
                .split(area);
            render_word_cloud_asset(f, panel, rows[0]);
            render_prompt_asset(f, panel, rows[1], selected_line, selected_lines);
        }
        (true, false, false, true) => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            render_text_asset(f, panel, rows[0]);
            render_word_cloud_asset(f, panel, rows[1]);
        }
        (false, true, false, true) => {
            let sides = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            render_diagram_asset(f, panel, sides[0]);
            render_word_cloud_asset(f, panel, sides[1]);
        }
        (false, false, true, false) => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
                .split(area);
            render_prompt_asset(f, panel, rows[1], selected_line, selected_lines);
        }
        _ => {
            f.render_widget(
                Paragraph::new(Span::styled("(empty panel)", Style::default().fg(Color::DarkGray))),
                area,
            );
        }
    }
    None
}

fn render_image_asset(f: &mut Frame, panel: &Panel, area: Rect, iterm2_supported: bool) -> Option<PendingImage> {
    use crate::assets::AssetKind;
    let asset = panel.image()?;
    let AssetKind::Image { image } = &asset.kind else { return None };

    if area.width == 0 || area.height == 0 {
        return None;
    }

    if iterm2_supported {
        // Leave ratatui's own buffer untouched here — the real pixels are painted
        // out-of-band via a raw OSC 1337 escape sequence after this frame is flushed,
        // so ratatui must never write into this Rect (it would fight with the image).
        let png_bytes = match crate::iterm2::encode_png(image) {
            Ok(bytes) => bytes,
            Err(_) => return None,
        };
        let escape = crate::iterm2::image_escape_sequence(&png_bytes, area.width, area.height);
        return Some(PendingImage { x: area.x, y: area.y, escape });
    }

    let target_w = area.width as u32;
    let target_h = area.height as u32 * 2;
    let scaled = image.resize(target_w, target_h, image::imageops::FilterType::Nearest);
    let rgba = scaled.to_rgba8();
    let w = rgba.width();
    let h = rgba.height();

    let mut lines: Vec<ratatui::text::Line<'static>> = Vec::new();
    let mut row = 0u32;
    while row < h {
        let mut spans: Vec<ratatui::text::Span<'static>> = Vec::new();
        for col in 0..w {
            let top = rgba.get_pixel(col, row);
            let bottom = if row + 1 < h {
                *rgba.get_pixel(col, row + 1)
            } else {
                image::Rgba([0, 0, 0, 255])
            };
            spans.push(ratatui::text::Span::styled(
                "▀",
                ratatui::style::Style::default()
                    .fg(Color::Rgb(top[0], top[1], top[2]))
                    .bg(Color::Rgb(bottom[0], bottom[1], bottom[2])),
            ));
        }
        lines.push(ratatui::text::Line::from(spans));
        row += 2;
    }

    f.render_widget(Paragraph::new(lines), area);
    None
}

fn render_text_asset(f: &mut Frame, panel: &Panel, area: Rect) {
    let Some(asset) = panel.assets.iter().find(|a| matches!(a.kind, AssetKind::Text { .. })) else { return };
    let AssetKind::Text { content } = &asset.kind else { return };
    let lines = crate::markdown::render_to_lines(content);
    f.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)))
            .wrap(Wrap { trim: false }),
        area,
    );
}

/// How many terminal cells wide each character is drawn, per word-cloud size tier.
fn word_cloud_char_repeat(size: WordCloudSize) -> u16 {
    match size {
        WordCloudSize::Small | WordCloudSize::Medium => 1,
        WordCloudSize::Large => 2,
    }
}

fn word_cloud_is_bold(size: WordCloudSize) -> bool {
    !matches!(size, WordCloudSize::Small)
}

fn render_word_cloud_asset(f: &mut Frame, panel: &Panel, area: Rect) {
    let Some(asset) = panel.word_cloud() else { return };
    let AssetKind::WordCloud { title, words, size } = &asset.kind else { return };

    f.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(format!(" {} ", title)),
        area,
    );

    if words.is_empty() {
        return;
    }

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let repeat = word_cloud_char_repeat(*size);
    let bold = word_cloud_is_bold(*size);
    let buf = f.buffer_mut();

    for placed in place_words(words, repeat, inner.width, inner.height) {
        let wh = word_hash(&placed.word);
        let x = inner.x + placed.x;
        let y = inner.y + placed.y;

        let color = CLOUD_COLORS[(wh as usize) % CLOUD_COLORS.len()];
        let style = if bold {
            Style::default().fg(color).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(color)
        };

        for (col, ch) in placed.word.chars().enumerate() {
            for r in 0..repeat {
                let cx = x + col as u16 * repeat + r;
                if cx < inner.x + inner.width {
                    if let Some(cell) = buf.cell_mut((cx, y)) {
                        cell.set_char(ch);
                        cell.set_style(style);
                        cell.set_diff_option(ratatui::buffer::CellDiffOption::None);
                    }
                }
            }
        }
    }
}

fn render_diagram_asset(f: &mut Frame, panel: &Panel, area: Rect) {
    let Some(asset) = panel.assets.iter().find(|a| matches!(a.kind, AssetKind::Diagram { .. })) else { return };
    let AssetKind::Diagram { content } = &asset.kind else { return };
    let (diagram_src, description) = crate::mermaid::parse(content);
    let mut lines = match diagram_src {
        Some(src) => crate::mermaid::render_to_lines(src),
        None => vec![],
    };
    if !description.is_empty() {
        lines.push(Line::from(""));
        for dl in description.lines() {
            lines.push(Line::from(Span::styled(dl.to_string(), Style::default().fg(Color::DarkGray))));
        }
    }

    let inner_w = area.width.saturating_sub(2);
    let inner_h = area.height.saturating_sub(2);
    let lines = crate::mermaid::center_lines(lines, inner_w, inner_h);

    f.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray))
                    .title(" Diagram "),
            ),
        area,
    );
}

fn render_prompt_asset(f: &mut Frame, panel: &Panel, area: Rect, selected_line: usize, selected_lines: &HashSet<usize>) {
    let Some(asset) = panel.prompt() else { return };
    let AssetKind::Prompt { label, content, sent } = &asset.kind else { return };
    let title = if *sent { format!(" {} ✓ ", label) } else { format!(" {} ", label) };
    let non_blank: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    let lines: Vec<Line> = non_blank.iter().enumerate().map(|(i, l)| {
        let is_cursor = i == selected_line;
        let is_selected = selected_lines.contains(&i);
        match (is_cursor, is_selected) {
            (true, true) => Line::from(vec![
                Span::styled("✓ ", Style::default().fg(Color::Green).bg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled(l.to_string(), Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD)),
            ]),
            (true, false) => Line::from(vec![
                Span::styled("> ", Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled(l.to_string(), Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD)),
            ]),
            (false, true) => Line::from(vec![
                Span::styled("✓ ", Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::styled(l.to_string(), Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD)),
            ]),
            (false, false) => Line::from(Span::styled(format!("  {l}"), Style::default().fg(Color::Yellow))),
        }
    }).collect();
    f.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow))
                    .title(title),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let topic = &app.topics[app.current_topic];
    let total_topics = app.topics.len();
    let current_topic = app.current_topic + 1;
    let total_panels = topic.panels.len();
    let current_panel = topic.current_panel + 1;

    f.render_widget(
        Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(Color::DarkGray)),
        area,
    );

    let inner = Rect { height: area.height.saturating_sub(1), ..area };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(inner);

    let big_title = BigText::builder()
        .pixel_size(PixelSize::Sextant)
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .lines(vec![Line::from(topic.label.clone())])
        .centered()
        .build();
    f.render_widget(big_title, rows[0]);

    f.render_widget(
        Paragraph::new(Span::styled(
            format!(" TOPIC {current_topic} of {total_topics}  Panel {current_panel} of {total_panels}"),
            Style::default().fg(Color::DarkGray),
        )),
        rows[1],
    );
}

fn render_status(f: &mut Frame, app: &App, _has_prompt: bool, area: Rect) {
    let msg = if let Some(status) = &app.status_message {
        status.clone()
    } else {
        "SPACE/l: next  c: copy path  R: reset  ?: help".to_string()
    };

    f.render_widget(
        Paragraph::new(Span::styled(format!(" {msg}"), Style::default().fg(Color::DarkGray))),
        area,
    );
}

fn render_help(f: &mut Frame) {
    let area = f.area();
    let popup = centered_rect(50, 20, area);
    f.render_widget(Clear, popup);
    f.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .title(" Help "),
        popup,
    );

    let inner = Rect {
        x: popup.x + 2,
        y: popup.y + 1,
        width: popup.width.saturating_sub(4),
        height: popup.height.saturating_sub(2),
    };

    let key = |k: &'static str| Span::styled(k, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    let desc = |d: &'static str| Span::styled(d, Style::default().fg(Color::White));
    let sep = || Span::raw("  ");

    f.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(vec![key("SPACE / l"), sep(), desc("Next")]),
            Line::from(vec![key("h"),         sep(), desc("Previous")]),
            Line::from(vec![key("→"),          sep(), desc("Next topic")]),
            Line::from(vec![key("←"),          sep(), desc("Previous topic")]),
            Line::from(""),
            Line::from(vec![key("j / k"),     sep(), desc("Move prompt cursor")]),
            Line::from(vec![key("c"),          sep(), desc("Toggle select line")]),
            Line::from(vec![key("s"),          sep(), desc("Send selection (or cursor line)")]),
            Line::from(vec![key("S"),          sep(), desc("Send all lines")]),
            Line::from(""),
            Line::from(vec![key("C"),          sep(), desc("Copy text file path")]),
            Line::from(vec![key("R"),          sep(), desc("Reset to start")]),
            Line::from(vec![key("q"),          sep(), desc("Quit")]),
            Line::from(""),
            Line::from(vec![key("?  Esc"),     sep(), desc("Close this help")]),
        ]),
        inner,
    );
}

fn render_confirm(f: &mut Frame, app: &App) {
    let area = f.area();
    let popup = centered_rect(44, 9, area);
    f.render_widget(Clear, popup);
    f.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
            .title(" Send Prompt "),
        popup,
    );

    let inner = Rect {
        x: popup.x + 2,
        y: popup.y + 1,
        width: popup.width.saturating_sub(4),
        height: popup.height.saturating_sub(2),
    };

    let label = &app.pending_label;
    let truncated = if label.len() > inner.width as usize - 2 {
        format!("{}…", &label[..inner.width as usize - 3])
    } else {
        label.clone()
    };

    f.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(truncated, Style::default().fg(Color::White).add_modifier(Modifier::BOLD))),
            Line::from(""),
            Line::from(""),
            Line::from(vec![
                Span::styled("  [ GO ]  ", Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::raw("   "),
                Span::styled(" cancel ", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(""),
            Line::from(Span::styled("Enter/g: send    Esc/n: cancel", Style::default().fg(Color::DarkGray))),
        ]),
        inner,
    );
}

fn render_countdown(f: &mut Frame, app: &App) {
    let area = f.area();
    let remaining = app.countdown_remaining();
    let popup = centered_rect(36, 11, area);
    f.render_widget(Clear, popup);
    f.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD))
            .title(" Focus Claude! "),
        popup,
    );

    let (digit_color, digit) = match remaining {
        3 => (Color::Yellow,  "3"),
        2 => (Color::Magenta, "2"),
        1 => (Color::Red,     "1"),
        _ => (Color::Green,   "→"),
    };

    let inner = Rect {
        x: popup.x + 2,
        y: popup.y + 1,
        width: popup.width.saturating_sub(4),
        height: popup.height.saturating_sub(2),
    };

    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled("Switch to Claude now!", Style::default().fg(Color::White).add_modifier(Modifier::BOLD))),
            Line::from(""),
            Line::from(Span::styled(digit, Style::default().fg(digit_color).add_modifier(Modifier::BOLD))),
            Line::from(""),
            Line::from(""),
            Line::from(""),
            Line::from(""),
            Line::from(Span::styled("Esc: cancel", Style::default().fg(Color::DarkGray))),
        ])
        .alignment(Alignment::Center),
        inner,
    );
}

fn render_firework(f: &mut Frame, fw: &Firework, area: Rect) {
    let cx = area.x as f64 + area.width as f64 / 2.0;
    let cy = area.y as f64 + area.height as f64 / 2.0;
    let buf = f.buffer_mut();
    for p in &fw.particles {
        let x = (cx + p.x * 9.0).round() as i32;
        let y = (cy + p.y * 4.5).round() as i32;
        if x >= area.x as i32
            && x < (area.x + area.width) as i32
            && y >= area.y as i32
            && y < (area.y + area.height) as i32
        {
            let color = ansi_to_color(COLORS[p.color_idx]);
            if let Some(cell) = buf.cell_mut((x as u16, y as u16)) {
                cell.set_char(p.ch);
                cell.set_fg(color);
                cell.set_diff_option(ratatui::buffer::CellDiffOption::None);
            }
        }
    }
}

fn ansi_to_color(ansi: u8) -> Color {
    match ansi {
        196 => Color::Red,
        208 | 226 => Color::Yellow,
        46 => Color::Green,
        51 => Color::Cyan,
        201 => Color::Magenta,
        _ => Color::White,
    }
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect { x, y, width: width.min(area.width), height: height.min(area.height) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::Asset;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::path::PathBuf;

    fn text_and_word_cloud_panel(text: &str, cloud_title: &str, words: &[&str]) -> Panel {
        word_cloud_panel_with_size(text, cloud_title, words, WordCloudSize::default())
    }

    fn word_cloud_panel_with_size(text: &str, cloud_title: &str, words: &[&str], size: WordCloudSize) -> Panel {
        let text_asset = Asset {
            path: PathBuf::from("text.md"),
            kind: AssetKind::Text { content: text.to_string() },
        };
        let cloud_asset = Asset {
            path: PathBuf::from("word-cloud.md"),
            kind: AssetKind::WordCloud {
                title: cloud_title.to_string(),
                words: words.iter().map(|w| w.to_string()).collect(),
                size,
            },
        };
        Panel { assets: vec![text_asset, cloud_asset] }
    }

    fn row_text(buffer: &ratatui::buffer::Buffer, y: u16) -> String {
        (0..buffer.area.width)
            .map(|x| buffer.cell((x, y)).map(|c| c.symbol()).unwrap_or(" ").to_string())
            .collect()
    }

    fn image_only_panel() -> Panel {
        use image::{DynamicImage, ImageBuffer, Rgb};
        let img = DynamicImage::ImageRgb8(ImageBuffer::from_fn(4, 4, |_, _| Rgb([200, 100, 50])));
        let asset = Asset {
            path: PathBuf::from("image.png"),
            kind: AssetKind::Image { image: img },
        };
        Panel { assets: vec![asset] }
    }

    fn text_only_panel(content: &str) -> Panel {
        let asset = Asset {
            path: PathBuf::from("text.md"),
            kind: AssetKind::Text { content: content.to_string() },
        };
        Panel { assets: vec![asset] }
    }

    #[test]
    fn image_asset_uses_iterm2_escape_when_supported() {
        let panel = image_only_panel();
        let backend = TestBackend::new(20, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut pending = None;
        terminal
            .draw(|f| {
                let area = f.area();
                pending = render_image_asset(f, &panel, area, true);
            })
            .unwrap();

        let pending = pending.expect("should return a PendingImage when iTerm2 is supported");
        assert_eq!(pending.x, 0);
        assert_eq!(pending.y, 0);
        assert!(
            pending.escape.starts_with("\x1b]1337;File=inline=1;"),
            "expected an OSC 1337 escape sequence, got: {:?}",
            pending.escape
        );

        // The actual pixels are painted out-of-band via the raw escape sequence, not
        // through ratatui, so ratatui's own buffer for that area should stay untouched.
        let buffer = terminal.backend().buffer();
        let row = row_text(buffer, 0);
        assert!(row.trim().is_empty(), "ratatui buffer should stay blank for the image area, got: {row:?}");
    }

    #[test]
    fn image_asset_falls_back_to_halfblock_when_not_supported() {
        let panel = image_only_panel();
        let backend = TestBackend::new(20, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut pending = None;
        terminal
            .draw(|f| {
                let area = f.area();
                pending = render_image_asset(f, &panel, area, false);
            })
            .unwrap();

        assert!(pending.is_none(), "should not return a pending image when iTerm2 isn't supported");

        let buffer = terminal.backend().buffer();
        let full_text: String = (0..buffer.area.height).map(|y| row_text(buffer, y)).collect();
        assert!(full_text.contains('▀'), "expected the halfblock fallback rendering, got: {full_text:?}");
    }

    #[test]
    fn render_panel_returns_pending_image_for_image_only_panel_in_iterm2_mode() {
        let panel = image_only_panel();
        let backend = TestBackend::new(20, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let selected_lines = HashSet::new();
        let mut pending = None;
        terminal
            .draw(|f| {
                let area = f.area();
                pending = render_panel(f, &panel, area, 0, &selected_lines, true);
            })
            .unwrap();
        assert!(pending.is_some());
    }

    #[test]
    fn render_panel_returns_none_for_text_only_panel_regardless_of_iterm2_flag() {
        let panel = text_only_panel("hello");
        let backend = TestBackend::new(20, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let selected_lines = HashSet::new();
        let mut pending = None;
        terminal
            .draw(|f| {
                let area = f.area();
                pending = render_panel(f, &panel, area, 0, &selected_lines, true);
            })
            .unwrap();
        assert!(pending.is_none());
    }

    #[test]
    fn text_and_word_cloud_stack_vertically_with_text_on_top() {
        let panel = text_and_word_cloud_panel("Panel body text", "My Cloud", &["alpha"]);
        let height = 20u16;
        let backend = TestBackend::new(60, height);
        let mut terminal = Terminal::new(backend).unwrap();
        let selected_lines = HashSet::new();
        terminal
            .draw(|f| {
                let area = f.area();
                render_panel(f, &panel, area, 0, &selected_lines, false);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();

        let top_row = row_text(buffer, 0);
        assert!(
            !top_row.contains("My Cloud"),
            "word cloud title should not be on the very top row when stacked with text: {top_row}"
        );

        let bottom_start = height / 2;
        let found_in_bottom = (bottom_start..height).any(|y| row_text(buffer, y).contains("My Cloud"));
        assert!(found_in_bottom, "word cloud title should appear in the bottom half of the panel");

        let found_text_in_top = (0..bottom_start).any(|y| row_text(buffer, y).contains("Panel body text"));
        assert!(found_text_in_top, "text content should appear in the top half of the panel");
    }

    fn single_word_cloud_panel(word: &str, size: WordCloudSize) -> Panel {
        word_cloud_panel(&[word], size)
    }

    fn word_cloud_panel(words: &[&str], size: WordCloudSize) -> Panel {
        let cloud_asset = Asset {
            path: PathBuf::from("word-cloud.md"),
            kind: AssetKind::WordCloud {
                title: "Cloud".to_string(),
                words: words.iter().map(|w| w.to_string()).collect(),
                size,
            },
        };
        Panel { assets: vec![cloud_asset] }
    }

    fn render_word_cloud_to_buffer(panel: &Panel) -> ratatui::buffer::Buffer {
        let backend = TestBackend::new(40, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let area = f.area();
                render_word_cloud_asset(f, panel, area);
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn count_symbol(buffer: &ratatui::buffer::Buffer, symbol: &str) -> usize {
        (0..buffer.area.height)
            .flat_map(|y| (0..buffer.area.width).map(move |x| (x, y)))
            .filter(|&(x, y)| buffer.cell((x, y)).map(|c| c.symbol() == symbol).unwrap_or(false))
            .count()
    }

    fn all_symbol_cells_bold(buffer: &ratatui::buffer::Buffer, symbol: &str) -> bool {
        (0..buffer.area.height)
            .flat_map(|y| (0..buffer.area.width).map(move |x| (x, y)))
            .filter(|&(x, y)| buffer.cell((x, y)).map(|c| c.symbol() == symbol).unwrap_or(false))
            .all(|(x, y)| {
                buffer
                    .cell((x, y))
                    .map(|c| c.modifier.contains(Modifier::BOLD))
                    .unwrap_or(false)
            })
    }

    #[test]
    fn small_word_cloud_renders_single_width_unbold() {
        let panel = single_word_cloud_panel("hi", WordCloudSize::Small);
        let buffer = render_word_cloud_to_buffer(&panel);
        assert_eq!(count_symbol(&buffer, "h"), 1);
        assert_eq!(count_symbol(&buffer, "i"), 1);
        assert!(!all_symbol_cells_bold(&buffer, "h"), "small word cloud should not be bold");
    }

    #[test]
    fn medium_word_cloud_renders_single_width_bold() {
        let panel = single_word_cloud_panel("hi", WordCloudSize::Medium);
        let buffer = render_word_cloud_to_buffer(&panel);
        assert_eq!(count_symbol(&buffer, "h"), 1);
        assert_eq!(count_symbol(&buffer, "i"), 1);
        assert!(all_symbol_cells_bold(&buffer, "h"), "medium word cloud should be bold");
    }

    #[test]
    fn large_word_cloud_renders_double_width_bold() {
        let panel = single_word_cloud_panel("hi", WordCloudSize::Large);
        let buffer = render_word_cloud_to_buffer(&panel);
        assert_eq!(count_symbol(&buffer, "h"), 2, "each character should be doubled for a large cloud");
        assert_eq!(count_symbol(&buffer, "i"), 2, "each character should be doubled for a large cloud");
        assert!(all_symbol_cells_bold(&buffer, "h"), "large word cloud should be bold");
    }

    #[test]
    fn colliding_words_render_intact_without_overwriting_each_other() {
        // Empirically confirmed (via an offline replay of cloud_position/word_hash) to
        // land on the same row with overlapping columns under the old per-word placement,
        // where "and" (rendered second) partially overwrites "Full" (rendered first). The
        // area is roomy (30x8 inner) so the fixed placer has an obvious free spot to find —
        // this isolates "does collision avoidance work" from "is the area too cramped for
        // both words to fit at all" (covered separately by the small-area drop tests).
        // Outer area adds 2 in each dimension for the 1-cell border.
        let panel = word_cloud_panel(&["Full", "and"], WordCloudSize::Medium);
        let backend = TestBackend::new(32, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let area = f.area();
                render_word_cloud_asset(f, &panel, area);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let full_text: String = (0..buffer.area.height)
            .map(|y| row_text(buffer, y))
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            full_text.contains("Full"),
            "expected \"Full\" to render intact without being overwritten by an overlapping word, got:\n{full_text}"
        );
        assert!(
            full_text.contains("and"),
            "expected \"and\" to render intact, got:\n{full_text}"
        );
    }

    fn word_list(n: usize, prefix: &str) -> Vec<String> {
        (0..n).map(|i| format!("{prefix}{i}")).collect()
    }

    /// Reconstructs a placed word's reserved footprint (including padding) for test
    /// assertions. `PlacedWord` itself only stores the draw origin, since production
    /// rendering never needs the footprint rect after placement is decided.
    fn placed_footprint(p: &PlacedWord, repeat: u16) -> Rect {
        Rect {
            x: p.x - WORD_PADDING,
            y: p.y - WORD_PADDING,
            width: p.word.chars().count() as u16 * repeat + WORD_PADDING * 2,
            height: 1 + WORD_PADDING * 2,
        }
    }

    #[test]
    fn place_words_never_produces_overlapping_rectangles() {
        let words = word_list(30, "sizeablewordnumber");
        let placed = place_words(&words, 1, 60, 20);
        for i in 0..placed.len() {
            for j in (i + 1)..placed.len() {
                let a = placed_footprint(&placed[i], 1);
                let b = placed_footprint(&placed[j], 1);
                assert!(
                    !a.intersects(b),
                    "placed words {:?} and {:?} overlap: {:?} vs {:?}",
                    placed[i].word,
                    placed[j].word,
                    a,
                    b
                );
            }
        }
    }

    #[test]
    fn place_words_is_deterministic() {
        let words = word_list(15, "word");
        let a = place_words(&words, 1, 40, 15);
        let b = place_words(&words, 1, 40, 15);
        let a_positions: Vec<(String, u16, u16)> =
            a.iter().map(|p| (p.word.clone(), p.x, p.y)).collect();
        let b_positions: Vec<(String, u16, u16)> =
            b.iter().map(|p| (p.word.clone(), p.x, p.y)).collect();
        assert_eq!(a_positions, b_positions);
    }

    #[test]
    fn place_words_drops_words_that_dont_fit_in_a_small_area() {
        let words = word_list(30, "w");
        let placed = place_words(&words, 1, 15, 3);
        assert!(
            placed.len() < words.len(),
            "expected some words to be dropped in a crowded small area, got {} of {}",
            placed.len(),
            words.len()
        );
    }

    #[test]
    fn place_words_still_fits_tiny_words_despite_a_flood_of_larger_ones() {
        // Placement order is size-first, so these two tiny words are actually placed
        // LAST, after every larger word has already claimed space -- they still get
        // placed because the exhaustive fallback scan finds whatever small gaps
        // remain, rather than giving up after a few unlucky random guesses.
        let mut words = word_list(2, "p");
        words.extend(word_list(60, "extracontenderword"));
        let placed = place_words(&words, 1, 30, 6);
        let placed_words: HashSet<&str> = placed.iter().map(|p| p.word.as_str()).collect();
        assert!(placed_words.contains(words[0].as_str()), "tiny word should still find a gap");
        assert!(placed_words.contains(words[1].as_str()), "tiny word should still find a gap");
        assert!(placed.len() < words.len(), "the flood of larger words should oversubscribe the area");
    }

    #[test]
    fn place_words_places_larger_words_before_smaller_ones_when_space_is_tight() {
        // Two sizeable words are listed LAST, after a flood of tiny competing words,
        // but placement order is by descending footprint size (not list order), so
        // they get first pick of space and are always placed -- placing big items
        // while the canvas is open is what lets more words fit overall. The area has
        // comfortable room for the two big words regardless of exactly where either
        // lands (their combined footprint is a small fraction of the total area), so
        // this isolates "does size-first ordering work" from "is there literally only
        // one possible non-overlapping arrangement" (a different, tighter scenario).
        let mut words = word_list(60, "tinyword");
        words.push("a-fairly-long-word-entry-one".to_string());
        words.push("a-fairly-long-word-entry-two".to_string());
        let placed = place_words(&words, 1, 70, 8);
        let placed_words: HashSet<&str> = placed.iter().map(|p| p.word.as_str()).collect();
        assert!(placed_words.contains("a-fairly-long-word-entry-one"), "larger word should be placed first");
        assert!(placed_words.contains("a-fairly-long-word-entry-two"), "larger word should be placed first");
        assert!(placed.len() < words.len(), "not every tiny word should fit alongside the larger ones");
    }

    #[test]
    fn place_words_fits_all_entries_when_there_is_objectively_room() {
        // Reported bug: real word-cloud phrases dropped from a panel that visibly had
        // plenty of empty space -- proof it was a placement-luck issue, not genuine
        // crowding. The area below is roomy enough (150x16) for all four to coexist.
        let words: Vec<String> = vec![
            "write a swift app. yes.",
            "why is that helm file failing to deploy?",
            "What about an NFS driver?",
            "Bugs in other peoples code.",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        let placed = place_words(&words, 1, 100, 12);
        let placed_words: HashSet<&str> = placed.iter().map(|p| p.word.as_str()).collect();
        for word in &words {
            assert!(
                placed_words.contains(word.as_str()),
                "expected {word:?} to be placed given the area has room for all four, only got: {placed_words:?}"
            );
        }
    }

    #[test]
    fn word_hash_is_deterministic() {
        assert_eq!(word_hash("rust"), word_hash("rust"));
    }

    #[test]
    fn word_hash_differs_for_different_words() {
        assert_ne!(word_hash("rust"), word_hash("python"));
    }

    #[test]
    fn cloud_seed_is_deterministic() {
        let words = vec!["ownership".to_string(), "borrowing".to_string()];
        assert_eq!(cloud_seed(&words), cloud_seed(&words));
    }

    #[test]
    fn cloud_seed_differs_for_different_word_lists() {
        let a = vec!["rust".to_string()];
        let b = vec!["python".to_string()];
        assert_ne!(cloud_seed(&a), cloud_seed(&b));
    }

    #[test]
    fn cloud_seed_empty_list_does_not_panic() {
        let _ = cloud_seed(&[]);
    }

    #[test]
    fn cloud_position_is_deterministic() {
        let wh = word_hash("test");
        let (x1, y1) = cloud_position(12345, wh, 0, 100, 40);
        let (x2, y2) = cloud_position(12345, wh, 0, 100, 40);
        assert_eq!((x1, y1), (x2, y2));
    }

    #[test]
    fn cloud_position_is_center_biased() {
        let width = 100u64;
        let height = 40u64;
        let words: Vec<String> = (0..500).map(|i| format!("word{}", i)).collect();
        let seed = cloud_seed(&words);

        let mut center_count = 0u32;
        let mut edge_count = 0u32;

        for (i, word) in words.iter().enumerate() {
            let wh = word_hash(word);
            let (x, y) = cloud_position(seed, wh, i, width, height);

            let x_center = x >= width / 4 && x < 3 * width / 4;
            let y_center = y >= height / 4 && y < 3 * height / 4;
            if x_center && y_center {
                center_count += 1;
            }

            let x_edge = x < width / 4 || x >= 3 * width / 4;
            let y_edge = y < height / 4 || y >= 3 * height / 4;
            if x_edge && y_edge {
                edge_count += 1;
            }
        }

        assert!(
            center_count > edge_count,
            "center ({}) should have more words than corners ({})",
            center_count,
            edge_count
        );
    }
}
