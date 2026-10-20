//! Widget para una sola herramienta en el transcript.

use crate::state::{StyleToken, ToolItem, ToolStatus};
use crate::widgets::{Anim, Theme};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

fn status_token(status: ToolStatus) -> StyleToken {
    match status {
        ToolStatus::Pending => StyleToken::ToolPending,
        ToolStatus::Running => StyleToken::ToolRunning,
        ToolStatus::Success => StyleToken::ToolSuccess,
        ToolStatus::Error => StyleToken::ToolError,
        ToolStatus::Canceled => StyleToken::ToolCanceled,
        ToolStatus::AwaitingPermission => StyleToken::Warning,
    }
}

pub fn render(frame: &mut Frame, area: Rect, item: &ToolItem, anim: &Anim, theme: &Theme) {
    let status = item.status();
    let glyph = match status {
        ToolStatus::Success => theme.glyphs.tool_success,
        ToolStatus::Error => theme.glyphs.tool_error,
        _ => theme.glyphs.tool_pending,
    };
    let style = theme.style(status_token(status));

    // Cabecera: estado + nombre + anim si está girando.
    let mut header = vec![
        Span::styled(format!("{glyph} "), style),
        Span::styled(format!("{} ", item.name), theme.style(StyleToken::ToolName)),
    ];
    if item.is_spinning() {
        let spin = anim.render("…", theme);
        header.extend(spin.spans);
    }

    let mut lines = vec![Line::from(header)];

    if let Some(summary) = item.summary.as_deref() {
        lines.push(Line::from(Span::styled(
            format!("  {summary}"),
            theme.style(StyleToken::Muted),
        )));
    }

    match status {
        ToolStatus::AwaitingPermission => {
            lines.push(Line::from(Span::styled("  Awaiting permission…", style)))
        }
        ToolStatus::Running => lines.push(Line::from(Span::styled("  Running…", style))),
        ToolStatus::Canceled => lines.push(Line::from(Span::styled("  Canceled.", style))),
        ToolStatus::Pending => lines.push(Line::from(Span::styled("  Queued…", style))),
        ToolStatus::Success | ToolStatus::Error => {
            if let Some(output) = item.output.as_deref() {
                for (i, line) in output.lines().enumerate() {
                    if !item.expanded && i > 0 {
                        break;
                    }
                    lines.push(Line::from(Span::styled(
                        format!("  {line}"),
                        theme.style(StyleToken::Muted),
                    )));
                }
            }
        }
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

/// Renderer compacto de Crush para una llamada genérica y su salida.
pub fn render_crush(frame: &mut Frame, area: Rect, item: &ToolItem, theme: &Theme) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let (icon, label, style) = match item.status() {
        ToolStatus::Success => (
            theme.glyphs.tool_success,
            "DONE",
            theme.style(StyleToken::ToolSuccess),
        ),
        ToolStatus::Error => (
            theme.glyphs.tool_error,
            "FAILED",
            theme.style(StyleToken::ToolError),
        ),
        ToolStatus::Canceled => (
            theme.glyphs.tool_error,
            "CANCELED",
            theme.style(StyleToken::ToolCanceled),
        ),
        ToolStatus::AwaitingPermission => ('!', "PERMISSION", theme.style(StyleToken::Warning)),
        ToolStatus::Pending | ToolStatus::Running => (
            theme.glyphs.tool_pending,
            "RUNNING",
            theme.style(StyleToken::ToolRunning),
        ),
    };
    let rail = theme.style(StyleToken::Border);
    let mut lines = vec![Line::from(vec![
        Span::styled("╭─ ", rail),
        Span::styled(format!("{icon} "), style),
        Span::styled(item.name.clone(), theme.style(StyleToken::ToolName)),
        Span::styled(format!("  {label}"), style.add_modifier(Modifier::BOLD)),
    ])];
    if let Some(summary) = item.summary.as_deref().filter(|text| !text.is_empty()) {
        let detail_style = if looks_like_path(summary) {
            theme.style(StyleToken::Accent)
        } else {
            theme.style(StyleToken::Code)
        };
        lines.push(Line::from(vec![
            Span::styled("│  IN   ", rail),
            Span::styled(summary.to_string(), detail_style),
        ]));
    }

    match item.status() {
        ToolStatus::Pending | ToolStatus::Running => {
            lines.push(Line::from(Span::styled(
                "│  … waiting for output",
                theme.style(StyleToken::ToolRunning),
            )));
        }
        ToolStatus::AwaitingPermission => {
            lines.push(Line::from(Span::styled(
                "│  ⚠ permission required",
                theme.style(StyleToken::Warning),
            )));
        }
        ToolStatus::Canceled => lines.push(Line::from(Span::styled(
            "╰─ canceled",
            theme.style(StyleToken::ToolCanceled),
        ))),
        ToolStatus::Success | ToolStatus::Error => {
            if let Some(output) = item.output.as_deref() {
                let output_lines: Vec<_> = output.lines().collect();
                let limit = if item.expanded {
                    output_lines.len()
                } else {
                    output_lines.len().min(2)
                };
                for (index, line) in output_lines.iter().take(limit).enumerate() {
                    let line_style = if item.status() == ToolStatus::Error {
                        theme.style(StyleToken::ToolError)
                    } else {
                        output_style(line, &item.name, theme)
                    };
                    lines.push(Line::from(vec![
                        Span::styled(
                            if index == 0 {
                                "├─ OUT  "
                            } else {
                                "│       "
                            },
                            rail,
                        ),
                        Span::styled((*line).to_string(), line_style),
                    ]));
                }
                if !item.expanded && limit < output_lines.len() {
                    lines.push(Line::from(Span::styled(
                        format!(
                            "╰─ … {} more lines · Ctrl+O to expand",
                            output_lines.len() - limit
                        ),
                        theme.style(StyleToken::Muted),
                    )));
                }
            }
        }
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn looks_like_path(text: &str) -> bool {
    text.split_whitespace().any(|word| {
        let word = word.trim_matches(['"', '\'', '`', ',', ':', ')', '(']);
        word.contains('/') || word.contains('\\')
    })
}

fn output_style(line: &str, tool: &str, theme: &Theme) -> Style {
    if line.starts_with("@@") {
        theme.style(StyleToken::DiffHunk)
    } else if line.starts_with("+++") {
        theme.style(StyleToken::DiffInsert)
    } else if line.starts_with("---") {
        theme.style(StyleToken::DiffDelete)
    } else if line.starts_with('+') {
        theme.style(StyleToken::DiffInsert)
    } else if line.starts_with('-') {
        theme.style(StyleToken::DiffDelete)
    } else if looks_like_path(line) {
        theme.style(StyleToken::Accent)
    } else if matches!(tool, "view" | "edit" | "grep" | "glob") {
        theme.style(StyleToken::Code)
    } else {
        theme.style(StyleToken::Muted)
    }
}

/// Tool call de Pawan: encabezado, salida enmarcada y plegado de resultados.
/// Los colores corresponden al tema por defecto de la referencia.
pub fn render_pawan(frame: &mut Frame, area: Rect, item: &ToolItem, duration_ms: u64) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let accent_dim = Style::default().fg(Color::Rgb(0, 153, 153));
    let warning = Style::default().fg(Color::Rgb(255, 184, 108));
    let muted = Style::default().fg(Color::Rgb(136, 136, 136));
    let foreground = Style::default().fg(Color::White);
    let (icon, status_color) = match item.status() {
        ToolStatus::Success => ("✓", Color::Rgb(80, 250, 123)),
        ToolStatus::Error => ("✗", Color::Rgb(255, 85, 85)),
        ToolStatus::Canceled => ("×", Color::Rgb(136, 136, 136)),
        ToolStatus::AwaitingPermission => ("⚠", Color::Rgb(255, 184, 108)),
        ToolStatus::Pending | ToolStatus::Running => ("⚙", Color::Rgb(255, 184, 108)),
    };

    let mut lines = vec![Line::raw("")];
    if matches!(item.status(), ToolStatus::Pending | ToolStatus::Running) {
        lines.push(Line::from(vec![
            Span::styled("  ╭ ", accent_dim),
            Span::styled("⚙ ", warning),
            Span::styled(
                format!("running {}", item.name),
                warning.add_modifier(Modifier::BOLD),
            ),
            Span::styled(" …", muted),
        ]));
    } else {
        let mut header = vec![
            Span::styled("  ╭─ ", accent_dim),
            Span::styled(
                format!("{icon} "),
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                item.name.clone(),
                Style::default()
                    .fg(Color::Rgb(0, 204, 204))
                    .add_modifier(Modifier::BOLD),
            ),
        ];
        if let Some(summary) = item.summary.as_deref().filter(|s| !s.is_empty()) {
            header.push(Span::styled(format!(" {summary}"), muted));
        }
        header.push(Span::styled(
            format!("  · {duration_ms}ms"),
            Style::default().fg(Color::Rgb(42, 42, 42)),
        ));
        lines.push(Line::from(header));

        if let Some(output) = item.output.as_deref() {
            let result_lines = output.lines().collect::<Vec<_>>();
            let limit = if item.expanded { 40 } else { 6 };
            for line in result_lines.iter().take(limit) {
                lines.push(Line::from(vec![
                    Span::styled("  │ ", accent_dim),
                    Span::styled((*line).to_string(), foreground),
                ]));
            }
            if result_lines.len() > limit {
                lines.push(Line::from(vec![
                    Span::styled("  │ ", accent_dim),
                    Span::styled(
                        format!(
                            "… {} more lines — press e to expand",
                            result_lines.len() - limit
                        ),
                        muted.add_modifier(Modifier::ITALIC),
                    ),
                ]));
            }
        }
        lines.push(Line::styled("  ╰─", accent_dim));
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

/// Datos de una herramienta en el transcript de Martty.
pub struct MarttyToolCall<'a> {
    pub name: &'a str,
    pub title: &'a str,
    pub request: &'a str,
    pub output: &'a str,
    pub error: Option<&'a str>,
    pub agent: Option<&'a str>,
    pub running: bool,
    pub expanded: bool,
}

/// Renderiza el bloque compacto de tool de Martty, con request visible durante
/// la ejecución y una ventana de las últimas cuatro líneas al completarse.
pub fn render_martty(
    frame: &mut Frame,
    area: Rect,
    call: &MarttyToolCall<'_>,
    anim: &Anim,
    theme: &Theme,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let muted = theme.style(StyleToken::Muted);
    let error_style = theme.style(StyleToken::Error);
    let mut header = vec![
        if call.running {
            anim.glyph(super::spinner::BRAILLE, theme)
        } else {
            Span::styled(
                "⏺ ",
                if call.error.is_some() {
                    error_style
                } else {
                    theme.style(StyleToken::Success)
                },
            )
        },
        Span::styled(call.name.to_string(), theme.style(StyleToken::Heading)),
    ];
    if let Some(agent) = call.agent {
        header.push(Span::styled(format!(" · {agent}"), muted));
    }

    let request = if call.running {
        call.request.trim()
    } else {
        ""
    };
    if !call.running && !call.title.is_empty() {
        header.push(Span::styled(format!("  {}", call.title), muted));
    }
    let result_lines = call.output.lines().collect::<Vec<_>>();
    let has_more = result_lines.len() > 4;
    if has_more {
        header.push(Span::styled(if call.expanded { " ▾" } else { " ▸" }, muted));
    }

    let mut lines = vec![Line::raw(""), Line::from(header)];
    if !request.is_empty() {
        for (index, line) in request.lines().enumerate() {
            lines.push(Line::from(vec![
                Span::styled("│ ", theme.style(StyleToken::Border)),
                Span::styled(
                    if index == 0 {
                        "request  "
                    } else {
                        "          "
                    },
                    muted,
                ),
                Span::styled(line.to_string(), muted),
            ]));
        }
    }
    if let Some(error) = call.error {
        lines.extend(error.lines().map(|line| {
            Line::from(vec![
                Span::styled("│ ", error_style),
                Span::styled(line.to_string(), error_style),
            ])
        }));
    }
    if !call.running && !result_lines.is_empty() {
        let visible = if call.expanded || !has_more {
            result_lines.as_slice()
        } else {
            &result_lines[result_lines.len() - 4..]
        };
        let bar = if call.error.is_some() {
            error_style
        } else {
            theme.style(StyleToken::Border)
        };
        lines.extend(visible.iter().map(|line| {
            Line::from(vec![
                Span::styled("│ ", bar),
                Span::styled((*line).to_string(), muted),
            ])
        }));
        if has_more && !call.expanded {
            lines.push(Line::from(vec![
                Span::styled("│ ", bar),
                Span::styled(
                    format!("last 4/{} lines · click to expand", result_lines.len()),
                    muted,
                ),
            ]));
        }
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

/// Llamada compacta de Mindfork con argumentos y resultado plegables.
pub fn render_mindfork(
    frame: &mut Frame,
    area: Rect,
    name: &str,
    arguments: &str,
    result: &str,
    running: bool,
    expanded: bool,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let muted = Style::default().fg(Color::Rgb(133, 139, 146));
    let gold = Style::default().fg(Color::Rgb(220, 175, 97));
    let mut header = vec![
        Span::styled("⚒ ", gold),
        Span::styled(
            format!("{name}({arguments})"),
            gold.add_modifier(Modifier::BOLD),
        ),
    ];
    if running {
        header.push(Span::styled("  · running…", muted));
    } else if !expanded {
        header.push(Span::styled("  · Ctrl+O ▸", muted));
    }
    let mut lines = vec![Line::from(header)];
    if expanded {
        if !arguments.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("│ arguments: ", muted),
                Span::styled(arguments.to_string(), muted),
            ]));
        }
        if !result.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("│ result: ", muted),
                Span::styled(result.to_string(), muted),
            ]));
        }
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ToolItem;
    use crate::widgets::{Anim, Theme};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn buffer(item: &ToolItem, w: u16, h: u16) -> String {
        let backend = TestBackend::new(w, h);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f, f.area(), item, &Anim { frame: 0 }, &Theme::test()))
            .unwrap();
        term.backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn shows_name_and_success_glyph() {
        let mut t = ToolItem::new("c1", "view");
        t.start();
        t.set_result(false);
        let out = buffer(&t, 40, 3);
        assert!(out.contains("view"));
        assert!(out.contains('✓'));
    }

    #[test]
    fn error_uses_the_error_glyph() {
        let mut t = ToolItem::new("c1", "bash");
        t.set_result(true);
        assert!(buffer(&t, 40, 3).contains('×'));
    }

    #[test]
    fn awaiting_permission_shows_its_message() {
        let mut t = ToolItem::new("c1", "bash");
        t.await_permission();
        assert!(buffer(&t, 40, 3).contains("permission"));
    }

    #[test]
    fn crush_tool_card_separates_arguments_output_and_code() {
        let mut item = ToolItem::new("c1", "view");
        item.set_summary(r#"{"path":"src/main.rs"}"#);
        item.set_result(false);
        item.set_output("fn main() {}\n+added line");
        let mut term = Terminal::new(TestBackend::new(80, 8)).unwrap();
        term.draw(|f| render_crush(f, f.area(), &item, &Theme::test()))
            .unwrap();
        let buffer = term.backend().buffer();
        let text = buffer
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("╭─ ✓ view  DONE"));
        assert!(text.contains("IN"));
        assert!(text.contains("OUT"));
        assert!(text.contains("fn main()"));
        assert!(text.contains("+added line"));
        assert_eq!(
            buffer[(8, 1)].fg,
            Theme::test().style(StyleToken::Accent).fg.unwrap()
        );
        assert_eq!(
            buffer[(8, 3)].fg,
            Theme::test().style(StyleToken::DiffInsert).fg.unwrap()
        );
    }

    #[test]
    fn zero_size_does_not_panic() {
        let t = ToolItem::new("c1", "view");
        let _ = buffer(&t, 0, 0);
    }
}
