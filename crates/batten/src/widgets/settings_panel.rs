//! Panel de herramientas de Mindfork basado en `settings-tools-dark-en.png`.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

const BG: Color = Color::Rgb(15, 16, 19);
const FG: Color = Color::Rgb(201, 204, 210);
const MUTED: Color = Color::Rgb(133, 139, 146);
const BORDER: Color = Color::Rgb(54, 58, 66);
const BLUE: Color = Color::Rgb(121, 169, 219);
const GREEN: Color = Color::Rgb(111, 192, 130);
const AMBER: Color = Color::Rgb(230, 201, 154);

/// Vista de ajustes de herramientas de Mindfork con sidebar y controles.
pub fn render_mindfork_tools(frame: &mut Frame, area: Rect) {
    if area.width < 50 || area.height < 12 {
        return;
    }
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(MUTED))
        .title(" ⚙ Settings ")
        .style(Style::default().bg(BG));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);
    let [sidebar, content] =
        Layout::horizontal([Constraint::Length(24), Constraint::Min(1)]).areas(inner);
    let sections = [
        ("▸ Sections", ""),
        ("Model/server", "39"),
        ("Sampling", "60"),
        ("Tools", "33"),
        ("Plugins", "4"),
        ("Memory", "25"),
        ("Data", "1"),
        ("Profiles", "61"),
        ("Interface", "18"),
    ];
    let sidebar_lines = sections
        .iter()
        .map(|(label, count)| {
            let active = *label == "Tools";
            let style = if active {
                Style::default()
                    .fg(FG)
                    .bg(Color::Rgb(41, 43, 48))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(MUTED)
            };
            let pad = (sidebar.width as usize).saturating_sub(label.width() + count.width() + 2);
            Line::from(vec![
                Span::styled(
                    if active { "▎ " } else { "  " },
                    if active {
                        Style::default().fg(GREEN)
                    } else {
                        style
                    },
                ),
                Span::styled((*label).to_string(), style),
                Span::raw(" ".repeat(pad)),
                Span::styled((*count).to_string(), Style::default().fg(MUTED)),
            ])
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(sidebar_lines).style(Style::default().bg(BG)),
        sidebar,
    );
    frame.render_widget(
        Block::default()
            .borders(ratatui::widgets::Borders::RIGHT)
            .border_style(Style::default().fg(BORDER)),
        sidebar,
    );

    let [heading, rows, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(content);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("◆ ", Style::default().fg(AMBER)),
            Span::styled(
                "Tools",
                Style::default().fg(FG).add_modifier(Modifier::BOLD),
            ),
        ])),
        heading,
    );

    let content_lines = tools_rows(rows.width as usize);
    frame.render_widget(Paragraph::new(content_lines), rows);
    frame.render_widget(Paragraph::new(footer_line()), footer);
}

/// Variante clara de la vista de herramientas de Mindfork.
pub fn render_mindfork_tools_light(frame: &mut Frame, area: Rect) {
    render_mindfork_tools(frame, area);
    apply_mindfork_light(frame, area);
}

/// Vista Model/server de Mindfork con pestañas, grupos y valores.
pub fn render_mindfork_model(frame: &mut Frame, area: Rect) {
    if area.width < 50 || area.height < 12 {
        return;
    }
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(MUTED))
        .title(" ⚙ Settings ")
        .style(Style::default().bg(BG));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);
    let [sidebar, content] =
        Layout::horizontal([Constraint::Length(24), Constraint::Min(1)]).areas(inner);

    let sections = [
        ("▸ Sections", ""),
        ("Model/server", "39"),
        ("Sampling", "60"),
        ("Tools", "33"),
        ("Plugins", "4"),
        ("Memory", "25"),
        ("Data", "1"),
        ("Profiles", "61"),
        ("Interface", "18"),
    ];
    let side_lines = sections
        .iter()
        .map(|(label, count)| {
            let active = *label == "Model/server";
            let style = if active {
                Style::default()
                    .fg(FG)
                    .bg(Color::Rgb(41, 43, 48))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(MUTED)
            };
            let gap = usize::from(sidebar.width).saturating_sub(label.width() + count.width() + 2);
            Line::from(vec![
                Span::styled(
                    if active { "▎ " } else { "  " },
                    if active {
                        Style::default().fg(GREEN)
                    } else {
                        style
                    },
                ),
                Span::styled(*label, style),
                Span::raw(" ".repeat(gap)),
                Span::styled(*count, Style::default().fg(MUTED)),
            ])
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(side_lines).style(Style::default().bg(BG)),
        sidebar,
    );
    frame.render_widget(
        Block::default()
            .borders(ratatui::widgets::Borders::RIGHT)
            .border_style(Style::default().fg(BORDER)),
        sidebar,
    );

    let [heading, rows, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(content);
    let heading_line = Line::from(vec![
        Span::styled(
            "◆ Model/server",
            Style::default().fg(FG).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" ".repeat(heading.width.saturating_sub(27) as usize)),
        Span::styled("● chat: ready", Style::default().fg(GREEN)),
    ]);
    frame.render_widget(Paragraph::new(heading_line), heading);

    let mut lines = vec![Line::from(vec![
        Span::styled(
            "Assistant",
            Style::default().fg(AMBER).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "   │   Impersonation   │   Embeddings   │   Speech",
            Style::default().fg(MUTED),
        ),
    ])];
    lines.push(Line::from(vec![
        Span::styled("Mode", Style::default().fg(MUTED)),
        Span::raw("                         "),
        Span::styled("‹ managed ›", Style::default().fg(BLUE)),
    ]));
    let width = rows.width as usize;
    settings_group(
        &mut lines,
        "Server",
        width,
        &[
            (
                "llama-server binary",
                r"C:\llama.cpp\llama-server.exe",
                false,
            ),
            ("Host", "127.0.0.1", false),
            ("Port", "8000", false),
        ],
    );
    settings_group(
        &mut lines,
        "Model",
        width,
        &[
            (
                "GGUF model (-m)",
                r"C:\models\gemma-4-12B-it-Q5_K_M.gguf",
                true,
            ),
            ("Vision projector (--mmproj)", "—", false),
            ("Context (-c)", "16384", true),
            ("Template (--jinja)", "[x]", false),
        ],
    );
    settings_group(
        &mut lines,
        "Performance",
        width,
        &[
            ("GPU layers (-ngl)", "99", false),
            ("Batch (-b)", "", false),
            ("FlashAttn", "‹ auto ›", false),
            ("No mmap", "[ ]", false),
        ],
    );
    settings_group(
        &mut lines,
        "Speculative decoding",
        width,
        &[
            ("Spec. decode (--spec-type)", "‹ draft-simple ›", true),
            (
                "Draft model (-md)",
                r"C:\models\gemma-4-1B-it-Q8_0.gguf",
                false,
            ),
            ("Draft GPU layers (-ngld)", "99", false),
        ],
    );
    frame.render_widget(Paragraph::new(lines), rows);
    frame.render_widget(Paragraph::new(footer_line()), footer);
}

/// Variante clara de Model/server de Mindfork.
pub fn render_mindfork_model_light(frame: &mut Frame, area: Rect) {
    render_mindfork_model(frame, area);
    apply_mindfork_light(frame, area);
}

/// Convierte los tokens fijos de las reconstrucciones dark al tema claro documentado.
pub fn apply_mindfork_light(frame: &mut Frame, area: Rect) {
    let remap = |color| match color {
        Color::Rgb(15, 16, 19) | Color::Rgb(15, 17, 21) => Color::Rgb(250, 250, 252),
        Color::Rgb(23, 25, 30) => Color::Rgb(238, 239, 242),
        Color::Rgb(41, 43, 48) => Color::Rgb(220, 222, 226),
        Color::Rgb(33, 36, 42) => Color::Rgb(232, 234, 237),
        Color::Rgb(54, 58, 66) => Color::Rgb(178, 182, 190),
        Color::Rgb(133, 139, 146) | Color::Rgb(133, 139, 147) => Color::Rgb(91, 97, 107),
        Color::Rgb(201, 204, 210) => Color::Rgb(36, 39, 45),
        other => other,
    };
    let buffer = frame.buffer_mut();
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            let cell = &mut buffer[(x, y)];
            cell.set_fg(remap(cell.fg));
            cell.set_bg(remap(cell.bg));
        }
    }
}

fn settings_group(
    lines: &mut Vec<Line<'static>>,
    title: &str,
    width: usize,
    settings: &[(&str, &str, bool)],
) {
    lines.push(Line::from(vec![
        Span::styled(
            title.to_string(),
            Style::default().fg(MUTED).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "─".repeat(width.saturating_sub(title.width() + 2)),
            Style::default().fg(BORDER),
        ),
    ]));
    for (label, value, changed) in settings {
        lines.push(Line::from(vec![
            Span::styled(
                if *changed { "• " } else { "  " },
                if *changed {
                    Style::default().fg(AMBER)
                } else {
                    Style::default().fg(MUTED)
                },
            ),
            Span::styled((*label).to_string(), Style::default().fg(MUTED)),
            Span::raw(" ".repeat(18)),
            Span::styled(
                (*value).to_string(),
                if value.starts_with('‹') {
                    Style::default().fg(BLUE)
                } else if *value == "[x]" {
                    Style::default().fg(GREEN)
                } else {
                    Style::default().fg(FG)
                },
            ),
        ]));
    }
}

fn tools_rows(width: usize) -> Vec<Line<'static>> {
    let groups = [
        (
            "Agentic loop — 1/3",
            vec![
                ("Tool round limit", "8", false),
                ("Confirm dangerous calls", "[ ]", false),
                ("Subagent: token limit", "4096", false),
                ("Subagent: run time limit (s)", "600", false),
                ("Subagent: parallel runs", "1", false),
                ("Background runs (subagent/dialogue)", "[ ]", false),
                ("Subagent: report background runs", "[x]", true),
                ("Subagent: background runs at once", "2", false),
                ("Dialogue: run time limit (s)", "1800", false),
                ("Quit: wait for background work (s)", "", false),
            ],
        ),
        (
            "Web search — 1/3",
            vec![
                ("Web search", "[ ]", false),
                ("Fetch pages", "[x]", true),
                ("Allow local addresses", "[ ]", false),
                ("Search provider", "‹ automatically ›", false),
                ("Tavily API key", "not set", false),
                ("Tavily API key (env, opt.)", "—", false),
            ],
        ),
        (
            "Python — 2/3",
            vec![("Python mode", "‹ Wasmer sandbox ›", false)],
        ),
    ];
    let mut lines = Vec::new();
    for (group, settings) in groups {
        if !lines.is_empty() {
            lines.push(Line::raw(""));
        }
        let title_width = group.width();
        lines.push(Line::from(vec![
            Span::styled(
                group,
                Style::default().fg(MUTED).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "─".repeat(width.saturating_sub(title_width + 2)),
                Style::default().fg(BORDER),
            ),
        ]));
        for (label, value, enabled) in settings {
            let label_style = Style::default().fg(MUTED);
            let marker = if enabled { "• " } else { "  " };
            lines.push(Line::from(vec![
                Span::styled(
                    marker,
                    if enabled {
                        Style::default().fg(AMBER)
                    } else {
                        label_style
                    },
                ),
                Span::styled(label, label_style),
                Span::raw(" ".repeat(8)),
                Span::styled(value, value_style(value, enabled)),
            ]));
        }
    }
    lines
}

fn value_style(value: &str, enabled: bool) -> Style {
    if enabled {
        Style::default().fg(GREEN)
    } else if value.starts_with('‹') {
        Style::default().fg(BLUE)
    } else {
        Style::default().fg(FG)
    }
}

fn footer_line() -> Line<'static> {
    let key = Style::default().fg(MUTED).bg(Color::Rgb(33, 36, 42));
    let label = Style::default().fg(MUTED);
    Line::from(vec![
        Span::styled(" Tab/↑↓ ", key),
        Span::styled(" section   ", label),
        Span::styled(" Enter ", key),
        Span::styled(" parameters   ", label),
        Span::styled(" / ", key),
        Span::styled(" search   ", label),
        Span::styled(" Ctrl+Z/Y ", key),
        Span::styled(" undo/redo   ", label),
        Span::styled(" F1 ", key),
        Span::styled(" help   ", label),
        Span::styled(" Esc ", key),
        Span::styled(" close   ", label),
        Span::styled(" Ctrl+Q ", key),
        Span::styled(" quit", label),
    ])
}
