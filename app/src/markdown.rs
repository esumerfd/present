use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

struct TableState {
    head: Vec<String>,
    body_rows: Vec<Vec<String>>,
    current_row: Vec<String>,
    current_cell: String,
    in_head: bool,
}

impl TableState {
    fn new() -> Self {
        Self { head: vec![], body_rows: vec![], current_row: vec![], current_cell: String::new(), in_head: false }
    }
}

/// Render markdown content to Typst markup, for embedding in the PDF export.
/// Walks the same `pulldown-cmark` event stream as `render_to_lines`, but emits
/// Typst syntax instead of styled ratatui spans -- a second sink for the one parse,
/// keeping panel authoring semantics identical between what's shown live and printed.
pub fn render_to_typst(content: &str) -> String {
    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
    let parser = Parser::new_ext(content, options);

    let mut out = String::new();
    let mut in_code_block = false;
    let mut list_depth: usize = 0;
    let mut table: Option<TableState> = None;

    for event in parser {
        match event {
            Event::Start(Tag::Table(_)) => {
                table = Some(TableState::new());
            }
            Event::Start(Tag::TableHead) => {
                if let Some(ref mut tbl) = table {
                    tbl.in_head = true;
                }
            }
            Event::End(TagEnd::TableHead) => {
                if let Some(ref mut tbl) = table {
                    tbl.head = std::mem::take(&mut tbl.current_row);
                    tbl.in_head = false;
                }
            }
            Event::Start(Tag::TableRow) => {
                if let Some(ref mut tbl) = table {
                    tbl.current_row.clear();
                }
            }
            Event::End(TagEnd::TableRow) => {
                if let Some(ref mut tbl) = table {
                    if !tbl.in_head {
                        tbl.body_rows.push(std::mem::take(&mut tbl.current_row));
                    }
                }
            }
            Event::Start(Tag::TableCell) => {
                if let Some(ref mut tbl) = table {
                    tbl.current_cell.clear();
                }
            }
            Event::End(TagEnd::TableCell) => {
                if let Some(ref mut tbl) = table {
                    tbl.current_row.push(std::mem::take(&mut tbl.current_cell));
                }
            }
            Event::End(TagEnd::Table) => {
                if let Some(tbl) = table.take() {
                    out.push_str(&render_typst_table(&tbl.head, &tbl.body_rows));
                }
            }
            Event::Text(text) if table.is_some() => {
                table.as_mut().unwrap().current_cell.push_str(&text);
            }
            Event::Code(text) if table.is_some() => {
                table.as_mut().unwrap().current_cell.push_str(&text);
            }
            _ if table.is_some() => {}
            Event::Start(Tag::Heading { level, .. }) => {
                let marker = "=".repeat(heading_level_number(level));
                out.push_str(&marker);
                out.push(' ');
            }
            Event::End(TagEnd::Heading(_)) => {
                out.push_str("\n\n");
            }
            Event::End(TagEnd::Paragraph) => {
                out.push_str("\n\n");
            }
            Event::Start(Tag::Strong) => out.push('*'),
            Event::End(TagEnd::Strong) => out.push('*'),
            Event::Start(Tag::Emphasis) => out.push('_'),
            Event::End(TagEnd::Emphasis) => out.push('_'),
            Event::Start(Tag::List(_)) => list_depth += 1,
            Event::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
                if list_depth == 0 {
                    out.push('\n');
                }
            }
            Event::Start(Tag::Item) => {
                out.push_str(&"  ".repeat(list_depth.saturating_sub(1)));
                out.push_str("- ");
            }
            Event::End(TagEnd::Item) => out.push('\n'),
            Event::Start(Tag::CodeBlock(_)) => {
                in_code_block = true;
                out.push_str("```\n");
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                out.push_str("\n```\n\n");
            }
            Event::Code(text) => {
                out.push('`');
                out.push_str(&text);
                out.push('`');
            }
            Event::Text(text) => {
                if in_code_block {
                    out.push_str(&text);
                } else {
                    out.push_str(&escape_typst(&text));
                }
            }
            Event::SoftBreak | Event::HardBreak => out.push('\n'),
            Event::Rule => out.push_str("#line(length: 100%)\n\n"),
            _ => {}
        }
    }

    out
}

fn render_typst_table(head: &[String], rows: &[Vec<String>]) -> String {
    let num_cols = head.len().max(rows.iter().map(|r| r.len()).max().unwrap_or(0));
    if num_cols == 0 {
        return String::new();
    }

    let mut out = format!("#table(\n  columns: {num_cols},\n");
    for cell in head {
        out.push_str(&format!("  [*{}*],\n", escape_typst(cell)));
    }
    for row in rows {
        for c in 0..num_cols {
            let cell = row.get(c).map(String::as_str).unwrap_or("");
            out.push_str(&format!("  [{}],\n", escape_typst(cell)));
        }
    }
    out.push_str(")\n\n");
    out
}

fn heading_level_number(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Escapes Typst markup-mode special characters so arbitrary panel text can't be
/// misinterpreted as Typst syntax (e.g. a literal `#` starting a function call).
pub fn escape_typst(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if matches!(ch, '\\' | '*' | '_' | '`' | '#' | '@' | '$' | '[' | ']' | '<') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

pub fn render_to_lines(content: &str) -> Vec<Line<'static>> {
    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
    let parser = Parser::new_ext(content, options);

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut current: Vec<Span<'static>> = Vec::new();
    let mut style_stack: Vec<Style> = vec![Style::default().fg(Color::White)];
    let mut in_code_block = false;
    let mut list_depth: usize = 0;
    let mut is_list_item = false;
    let mut table: Option<TableState> = None;

    let current_style = |stack: &[Style]| *stack.last().unwrap_or(&Style::default());

    for event in parser {
        match event {
            // --- Table events ---
            Event::Start(Tag::Table(_)) => {
                if !current.is_empty() {
                    lines.push(Line::from(std::mem::take(&mut current)));
                }
                table = Some(TableState::new());
            }
            Event::Start(Tag::TableHead) => {
                if let Some(ref mut tbl) = table { tbl.in_head = true; }
            }
            Event::End(TagEnd::TableHead) => {
                if let Some(ref mut tbl) = table {
                    tbl.head = std::mem::take(&mut tbl.current_row);
                    tbl.in_head = false;
                }
            }
            Event::Start(Tag::TableRow) => {
                if let Some(ref mut tbl) = table { tbl.current_row.clear(); }
            }
            Event::End(TagEnd::TableRow) => {
                if let Some(ref mut tbl) = table {
                    if !tbl.in_head {
                        tbl.body_rows.push(std::mem::take(&mut tbl.current_row));
                    }
                }
            }
            Event::Start(Tag::TableCell) => {
                if let Some(ref mut tbl) = table { tbl.current_cell.clear(); }
            }
            Event::End(TagEnd::TableCell) => {
                if let Some(ref mut tbl) = table {
                    tbl.current_row.push(std::mem::take(&mut tbl.current_cell));
                }
            }
            Event::End(TagEnd::Table) => {
                if let Some(tbl) = table.take() {
                    lines.extend(render_table_lines(&tbl.head, &tbl.body_rows));
                }
            }
            Event::Text(text) if table.is_some() => {
                table.as_mut().unwrap().current_cell.push_str(&text);
            }
            Event::Code(text) if table.is_some() => {
                table.as_mut().unwrap().current_cell.push_str(&text);
            }
            _ if table.is_some() => {}
            // --- Normal events ---
            Event::Start(Tag::Heading { level, .. }) => {
                if !current.is_empty() {
                    lines.push(Line::from(std::mem::take(&mut current)));
                }
                lines.push(Line::from(""));
                let style = heading_style(level);
                style_stack.push(style);
            }
            Event::End(TagEnd::Heading(_)) => {
                style_stack.pop();
                lines.push(Line::from(std::mem::take(&mut current)));
                lines.push(Line::from(""));
            }
            Event::Start(Tag::Paragraph) => {
                if !current.is_empty() {
                    lines.push(Line::from(std::mem::take(&mut current)));
                    lines.push(Line::from(""));
                }
            }
            Event::End(TagEnd::Paragraph) => {
                if !current.is_empty() {
                    lines.push(Line::from(std::mem::take(&mut current)));
                }
                lines.push(Line::from(""));
            }
            Event::Start(Tag::Strong) => {
                let s = current_style(&style_stack).add_modifier(Modifier::BOLD);
                style_stack.push(s);
            }
            Event::End(TagEnd::Strong) => {
                style_stack.pop();
            }
            Event::Start(Tag::Emphasis) => {
                let s = current_style(&style_stack).add_modifier(Modifier::ITALIC);
                style_stack.push(s);
            }
            Event::End(TagEnd::Emphasis) => {
                style_stack.pop();
            }
            Event::Start(Tag::List(_)) => {
                list_depth += 1;
            }
            Event::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
                if list_depth == 0 {
                    lines.push(Line::from(""));
                }
            }
            Event::Start(Tag::Item) => {
                if !current.is_empty() {
                    lines.push(Line::from(std::mem::take(&mut current)));
                }
                let indent = "  ".repeat(list_depth.saturating_sub(1));
                current.push(Span::styled(
                    format!("{indent}• "),
                    Style::default().fg(Color::Yellow),
                ));
                is_list_item = true;
            }
            Event::End(TagEnd::Item) => {
                if !current.is_empty() {
                    lines.push(Line::from(std::mem::take(&mut current)));
                }
                is_list_item = false;
            }
            Event::Start(Tag::CodeBlock(_)) => {
                in_code_block = true;
                if !current.is_empty() {
                    lines.push(Line::from(std::mem::take(&mut current)));
                }
                lines.push(Line::from(""));
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                lines.push(Line::from(""));
            }
            Event::Code(text) => {
                current.push(Span::styled(
                    text.to_string(),
                    Style::default().fg(Color::Green),
                ));
            }
            Event::Text(text) => {
                if in_code_block {
                    for line in text.lines() {
                        lines.push(Line::from(Span::styled(
                            format!("  {line}"),
                            Style::default().fg(Color::Green),
                        )));
                    }
                } else {
                    let style = current_style(&style_stack);
                    for (i, segment) in text.split('\n').enumerate() {
                        if i > 0 {
                            lines.push(Line::from(std::mem::take(&mut current)));
                        }
                        if !segment.is_empty() {
                            current.push(Span::styled(segment.to_string(), style));
                        }
                    }
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                lines.push(Line::from(std::mem::take(&mut current)));
                if is_list_item {
                    let indent = "  ".repeat(list_depth.saturating_sub(1));
                    current.push(Span::raw(format!("{indent}  ")));
                }
            }
            Event::Rule => {
                lines.push(Line::from(Span::styled(
                    "─".repeat(40),
                    Style::default().fg(Color::DarkGray),
                )));
            }
            _ => {}
        }
    }

    if !current.is_empty() {
        lines.push(Line::from(current));
    }

    lines
}

fn render_table_lines(head: &[String], rows: &[Vec<String>]) -> Vec<Line<'static>> {
    let num_cols = head.len().max(rows.iter().map(|r| r.len()).max().unwrap_or(0));
    if num_cols == 0 {
        return vec![];
    }
    let col_widths: Vec<usize> = (0..num_cols)
        .map(|c| {
            let hw = head.get(c).map(|s| s.len()).unwrap_or(0);
            let bw = rows.iter().map(|r| r.get(c).map(|s| s.len()).unwrap_or(0)).max().unwrap_or(0);
            hw.max(bw).max(1)
        })
        .collect();

    let mut result = vec![Line::from("")];

    let mut header_spans: Vec<Span<'static>> = Vec::new();
    for c in 0..num_cols {
        let text = head.get(c).cloned().unwrap_or_default();
        header_spans.push(Span::styled(
            format!(" {:<width$} ", text, width = col_widths[c]),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ));
        if c + 1 < num_cols {
            header_spans.push(Span::styled("│", Style::default().fg(Color::DarkGray)));
        }
    }
    result.push(Line::from(header_spans));

    let mut sep_spans: Vec<Span<'static>> = Vec::new();
    for c in 0..num_cols {
        sep_spans.push(Span::styled(
            "─".repeat(col_widths[c] + 2),
            Style::default().fg(Color::DarkGray),
        ));
        if c + 1 < num_cols {
            sep_spans.push(Span::styled("┼", Style::default().fg(Color::DarkGray)));
        }
    }
    result.push(Line::from(sep_spans));

    for row in rows {
        let mut row_spans: Vec<Span<'static>> = Vec::new();
        for c in 0..num_cols {
            let text = row.get(c).cloned().unwrap_or_default();
            row_spans.push(Span::styled(
                format!(" {:<width$} ", text, width = col_widths[c]),
                Style::default().fg(Color::White),
            ));
            if c + 1 < num_cols {
                row_spans.push(Span::styled("│", Style::default().fg(Color::DarkGray)));
            }
        }
        result.push(Line::from(row_spans));
    }

    result.push(Line::from(""));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typst_heading_renders_as_equals_signs() {
        let out = render_to_typst("# Title\n\nBody text");
        assert!(out.contains("= Title"), "expected '= Title' heading in: {out:?}");
    }

    #[test]
    fn typst_h2_renders_with_two_equals_signs() {
        let out = render_to_typst("## Subtitle");
        assert!(out.contains("== Subtitle"), "expected '== Subtitle' in: {out:?}");
    }

    #[test]
    fn typst_bold_renders_with_asterisks() {
        let out = render_to_typst("**bold**");
        assert!(out.contains("*bold*"), "expected '*bold*' in: {out:?}");
    }

    #[test]
    fn typst_italic_renders_with_underscores() {
        let out = render_to_typst("*italic*");
        assert!(out.contains("_italic_"), "expected '_italic_' in: {out:?}");
    }

    #[test]
    fn typst_inline_code_renders_with_backticks() {
        let out = render_to_typst("`code`");
        assert!(out.contains("`code`"), "expected backtick-wrapped code in: {out:?}");
    }

    #[test]
    fn typst_list_items_render_with_dash_prefix() {
        let out = render_to_typst("- one\n- two");
        assert!(out.contains("- one"), "expected '- one' in: {out:?}");
        assert!(out.contains("- two"), "expected '- two' in: {out:?}");
    }

    #[test]
    fn typst_table_renders_as_a_hash_table_call() {
        let md = "| Key | Action |\n|-----|--------|\n| q | Quit |\n";
        let out = render_to_typst(md);
        assert!(out.contains("#table("), "expected a Typst table call in: {out:?}");
        assert!(out.contains("[*Key*]") && out.contains("[*Action*]"), "expected bold header cells in: {out:?}");
        assert!(out.contains("[q]") && out.contains("[Quit]"), "expected body cells in: {out:?}");
    }

    #[test]
    fn typst_special_characters_in_plain_text_are_escaped() {
        let out = render_to_typst("Use #hashtags and *not* bold via literal text");
        assert!(out.contains("\\#hashtags"), "expected escaped '#' in: {out:?}");
    }

    fn lines_to_strings(lines: &[Line]) -> Vec<String> {
        lines.iter().map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect()).collect()
    }

    #[test]
    fn renders_table_header_without_raw_pipes() {
        let md = "| Key | Action |\n|-----|--------|\n| q | Quit |\n";
        let lines = render_to_lines(md);
        let text = lines_to_strings(&lines);
        assert!(
            text.iter().any(|l| l.contains("Key") && l.contains("Action") && !l.starts_with("| ")),
            "header should be formatted without raw pipes, got: {text:?}"
        );
    }

    #[test]
    fn renders_table_separator_line() {
        let md = "| Key | Action |\n|-----|--------|\n| q | Quit |\n";
        let lines = render_to_lines(md);
        let text = lines_to_strings(&lines);
        assert!(text.iter().any(|l| l.contains('─')), "should have ─ separator, got: {text:?}");
    }

    #[test]
    fn renders_table_body_rows() {
        let md = "| Key | Action |\n|-----|--------|\n| q | Quit |\n";
        let lines = render_to_lines(md);
        let text = lines_to_strings(&lines);
        assert!(text.iter().any(|l| l.contains("Quit")), "should have Quit in body, got: {text:?}");
    }

    #[test]
    fn heading_is_followed_by_a_blank_line_before_the_next_paragraph() {
        let md = "# Title\n\nBody text\n";
        let lines = render_to_lines(md);
        let text = lines_to_strings(&lines);
        let heading_idx = text.iter().position(|l| l.contains("Title")).expect("heading not found");
        assert_eq!(
            text.get(heading_idx + 1).map(String::as_str),
            Some(""),
            "expected a blank line right after the heading, got: {text:?}"
        );
    }

    #[test]
    fn table_columns_are_padded_to_equal_width() {
        let md = "| A | B |\n|---|---|\n| short | a very long cell |\n";
        let lines = render_to_lines(md);
        let text = lines_to_strings(&lines);
        let header = text.iter().find(|l| l.contains("A") && l.contains("B") && !l.starts_with("| "))
            .expect("header line not found");
        // "B" column header should be padded to match "a very long cell" width
        let b_col_start = header.find('B').expect("B not found in header");
        assert!(header.len() > b_col_start + 1, "B column should be padded");
    }
}

fn heading_style(level: HeadingLevel) -> Style {
    match level {
        HeadingLevel::H1 => Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
        HeadingLevel::H2 => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        HeadingLevel::H3 => Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
        _ => Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    }
}
