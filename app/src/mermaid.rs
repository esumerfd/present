use mmdflux::{render_diagram, OutputFormat, RenderConfig};
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

/// Extract the mermaid source and optional description from a diagram.md file.
/// The file may contain a ```mermaid ... ``` fence plus free text below it.
pub fn parse(file_content: &str) -> (Option<&str>, &str) {
    if let Some(start) = file_content.find("```mermaid") {
        let after_fence = &file_content[start + "```mermaid".len()..];
        if let Some(end) = after_fence.find("```") {
            let diagram_src = after_fence[..end].trim();
            let description = after_fence[end + 3..].trim();
            return (Some(diagram_src), description);
        }
    }
    // No fence — treat entire content as raw diagram source
    (Some(file_content.trim()), "")
}

/// Render a mermaid diagram source to SVG markup, for embedding as a vector
/// graphic (e.g. in the PDF export). Returns `None` if mmdflux can't render it.
pub fn render_to_svg(diagram_src: &str) -> Option<String> {
    render_diagram(diagram_src, OutputFormat::Svg, &RenderConfig::default()).ok()
}

/// Render a mermaid diagram source to lines of styled ratatui text.
/// Falls back to displaying the raw source if mmdflux can't render it.
pub fn render_to_lines(diagram_src: &str) -> Vec<Line<'static>> {
    match render_diagram(diagram_src, OutputFormat::Text, &RenderConfig::default()) {
        Ok(rendered) => rendered
            .lines()
            .map(|l| Line::from(colorize(l)))
            .collect(),
        Err(_) => diagram_src
            .lines()
            .map(|l| Line::from(Span::styled(l.to_string(), Style::default().fg(Color::DarkGray))))
            .collect(),
    }
}

/// Center a block of lines within the given area by prepending horizontal and vertical whitespace.
/// The block is treated as a unit — each line gets the same left padding so the art stays intact.
pub fn center_lines(lines: Vec<Line<'static>>, inner_width: u16, inner_height: u16) -> Vec<Line<'static>> {
    if lines.is_empty() {
        return lines;
    }
    let max_width = lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.chars().count()).sum::<usize>())
        .max()
        .unwrap_or(0);

    let h_pad = (inner_width as usize).saturating_sub(max_width) / 2;
    let v_pad = (inner_height as usize).saturating_sub(lines.len()) / 2;

    let prefix = " ".repeat(h_pad);
    let mut result = Vec::with_capacity(v_pad + lines.len());
    for _ in 0..v_pad {
        result.push(Line::from(""));
    }
    for mut line in lines {
        if h_pad > 0 {
            line.spans.insert(0, Span::raw(prefix.clone()));
        }
        result.push(line);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_text(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn center_lines_adds_horizontal_padding() {
        let lines = vec![Line::from("ABC")];
        let result = center_lines(lines, 9, 1);
        let text = line_text(result.last().unwrap());
        assert!(text.starts_with("   "), "expected 3 spaces of h-padding, got: {text:?}");
        assert!(text.contains("ABC"));
    }

    #[test]
    fn center_lines_adds_vertical_padding() {
        let lines = vec![Line::from("ABC")];
        let result = center_lines(lines, 5, 5);
        assert_eq!(result.len(), 3, "1 content line + 2 blank top lines");
        assert!(line_text(&result[0]).trim().is_empty(), "first line should be blank");
        assert!(line_text(&result[1]).trim().is_empty(), "second line should be blank");
        assert!(line_text(&result[2]).contains("ABC"));
    }

    #[test]
    fn center_lines_no_negative_padding_when_content_wider_than_area() {
        let lines = vec![Line::from("A".repeat(100))];
        let result = center_lines(lines, 50, 5);
        let text = line_text(result.last().unwrap());
        assert!(text.contains("AAAA"), "content should be present");
        assert!(!text.starts_with(' '), "no padding when content wider than area");
    }

    #[test]
    fn center_lines_empty_input_returns_empty() {
        assert!(center_lines(vec![], 80, 24).is_empty());
    }

    #[test]
    fn render_to_svg_produces_svg_markup_for_valid_diagram() {
        let svg = render_to_svg("graph TD\n    A-->B");
        assert!(svg.is_some(), "expected Some(svg) for a valid diagram");
        assert!(svg.unwrap().contains("<svg"), "expected SVG markup in output");
    }

    #[test]
    fn render_to_svg_returns_none_for_unrenderable_diagram() {
        let svg = render_to_svg("not a mermaid diagram at all {{{");
        assert!(svg.is_none(), "expected None when mmdflux can't render the source");
    }
}

fn colorize(line: &str) -> Vec<Span<'static>> {
    let keyword = Style::default().fg(Color::Cyan);
    let arrow   = Style::default().fg(Color::Yellow);
    let label   = Style::default().fg(Color::Green);
    let default = Style::default().fg(Color::White);

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut rest = line;

    while !rest.is_empty() {
        // Pipe-delimited labels: |text|
        if let Some(pos) = rest.find('|') {
            let before = &rest[..pos];
            if !before.is_empty() {
                tokenize(before, keyword, arrow, default, &mut spans);
            }
            let after = &rest[pos + 1..];
            if let Some(end) = after.find('|') {
                spans.push(Span::styled(format!("|{}|", &after[..end]), label));
                rest = &after[end + 1..];
            } else {
                spans.push(Span::styled("|".to_string(), default));
                rest = after;
            }
            continue;
        }
        tokenize(rest, keyword, arrow, default, &mut spans);
        break;
    }

    if spans.is_empty() {
        spans.push(Span::styled(line.to_string(), default));
    }
    spans
}

fn tokenize(segment: &str, keyword: Style, arrow: Style, default: Style, out: &mut Vec<Span<'static>>) {
    let mut i = 0;
    let bytes = segment.as_bytes();

    while i < bytes.len() {
        if let Some((s, len)) = match_arrow(&segment[i..]) {
            out.push(Span::styled(s, arrow));
            i += len;
            continue;
        }

        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'-')
            {
                i += 1;
            }
            let word = &segment[start..i];
            let style = if is_keyword(word) { keyword } else { default };
            out.push(Span::styled(word.to_string(), style));
            continue;
        }

        let start = i;
        while i < segment.len() {
            let b = bytes[i];
            if b.is_ascii_alphabetic() || b == b'_' || b == b'|' {
                break;
            }
            if match_arrow(&segment[i..]).is_some() {
                break;
            }
            i += if b < 0x80 { 1 } else { segment[i..].chars().next().unwrap().len_utf8() };
        }
        if i > start {
            out.push(Span::styled(segment[start..i].to_string(), default));
        }
    }
}

fn match_arrow(s: &str) -> Option<(String, usize)> {
    for pat in &["-.->", "==>", "-->", "---", "-.-", "-..", "->", "--"] {
        if s.starts_with(pat) {
            return Some((pat.to_string(), pat.len()));
        }
    }
    None
}

fn is_keyword(word: &str) -> bool {
    matches!(
        word,
        "flowchart" | "graph" | "sequenceDiagram" | "classDiagram" | "stateDiagram"
            | "stateDiagram-v2" | "erDiagram" | "gantt" | "pie" | "journey" | "gitGraph"
            | "mindmap" | "timeline" | "TB" | "TD" | "BT" | "LR" | "RL"
            | "subgraph" | "end" | "section" | "title" | "participant" | "actor"
            | "loop" | "alt" | "else" | "opt" | "note" | "class" | "state"
    )
}
