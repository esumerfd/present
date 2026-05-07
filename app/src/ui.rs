use crate::app::{App, Screen};
use crate::assets::{AssetKind, Panel};
use crate::firework::{Firework, COLORS};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use tui_big_text::{BigText, PixelSize};


pub fn render(f: &mut Frame, app: &App) {
    match app.screen {
        Screen::Intro => render_intro(f, app),
        Screen::Topic => render_topic(f, app),
        Screen::Confirm => {
            render_topic(f, app);
            render_confirm(f, app);
        }
        Screen::Countdown => {
            render_topic(f, app);
            render_countdown(f, app);
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

fn render_topic(f: &mut Frame, app: &App) {
    let area = f.area();
    let Some(topic) = app.topics.get(app.current_topic) else { return };
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

    if let Some(panel) = topic.current_panel() {
        render_panel(f, panel, chunks[1]);
    }

    render_status(f, app, has_prompt, chunks[2]);

    if let Some(fw) = &app.firework {
        render_firework(f, fw, area);
    }
}

fn render_panel(f: &mut Frame, panel: &Panel, area: Rect) {
    let has_text    = panel.assets.iter().any(|a| matches!(a.kind, AssetKind::Text { .. }));
    let has_diagram = panel.assets.iter().any(|a| matches!(a.kind, AssetKind::Diagram { .. }));
    let has_prompt  = panel.has_prompt();

    match (has_text, has_diagram, has_prompt) {
        (true, true, _) => {
            let sides = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            render_text_asset(f, panel, sides[0]);
            render_diagram_asset(f, panel, sides[1]);
        }
        (true, false, true) => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
                .split(area);
            render_text_asset(f, panel, rows[0]);
            render_prompt_asset(f, panel, rows[1]);
        }
        (false, true, true) => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
                .split(area);
            render_diagram_asset(f, panel, rows[0]);
            render_prompt_asset(f, panel, rows[1]);
        }
        (_, true, false) => render_diagram_asset(f, panel, area),
        (true, false, false) => render_text_asset(f, panel, area),
        (false, false, true) => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
                .split(area);
            render_prompt_asset(f, panel, rows[1]);
        }
        _ => {
            f.render_widget(
                Paragraph::new(Span::styled("(empty panel)", Style::default().fg(Color::DarkGray))),
                area,
            );
        }
    }
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
    f.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray))
                    .title(" Diagram "),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_prompt_asset(f: &mut Frame, panel: &Panel, area: Rect) {
    let Some(asset) = panel.prompt() else { return };
    let AssetKind::Prompt { label, content, sent } = &asset.kind else { return };
    let title = if *sent { format!(" {} ✓ ", label) } else { format!(" {} ", label) };
    let lines: Vec<Line> = content
        .lines()
        .map(|l| Line::from(Span::styled(l.to_string(), Style::default().fg(Color::Yellow))))
        .collect();
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

fn render_status(f: &mut Frame, app: &App, has_prompt: bool, area: Rect) {
    let msg = if let Some(status) = &app.status_message {
        status.clone()
    } else if has_prompt {
        "SPACE/l: next panel  h: prev  →: next topic  ←: prev topic  s: send prompt  q: quit".to_string()
    } else {
        "SPACE/l: next panel  h: prev  →: next topic  ←: prev topic  q: quit".to_string()
    };

    f.render_widget(
        Paragraph::new(Span::styled(format!(" {msg}"), Style::default().fg(Color::DarkGray))),
        area,
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
                cell.set_skip(false);
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
