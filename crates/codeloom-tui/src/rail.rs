//! Rail derecho de contexto del look Crush: modelo, ficheros, herramientas,
//! jobs y subagentes de la sesión abierta.

use crate::app::App;
use batten::state::ToolStatus;
use codeloom_core::job::JobStatus;
use codeloom_core::session::SubagentStatus;
use codeloom_core::todo::TodoStatus;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

/// Máximo de tareas mostradas; el resto se resume en una línea.
const TODO_LIMIT: usize = 8;
/// Máximo de ficheros mostrados; el resto se resume en una línea.
const FILE_LIMIT: usize = 8;
/// Máximo de herramientas recientes mostradas.
const TOOL_LIMIT: usize = 6;
/// Máximo de jobs de la sesión mostrados.
const JOB_LIMIT: usize = 5;
/// Máximo de subagentes mostrados.
const SUBAGENT_LIMIT: usize = 5;

/// Dibuja el rail en `area`, separado del chat por un borde izquierdo.
/// El contenido se recorta al alto disponible; no hay desplazamiento.
pub fn draw_context_rail(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(app.theme.styles.border);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.is_empty() {
        return;
    }
    frame.render_widget(Paragraph::new(rail_lines(app)), inner);
}

/// Construye las líneas del rail. Cada lista se limita antes de construir
/// sus líneas, así que el coste no crece con el número de elementos.
fn rail_lines(app: &App) -> Vec<Line<'_>> {
    let mut out: Vec<Line<'_>> = Vec::new();

    out.push(title("CODELOOM", app));
    out.push(Line::from(Span::styled(
        app.cwd.as_str(),
        app.theme.styles.muted,
    )));

    push_gap(&mut out);
    out.push(title("MODEL", app));
    out.push(Line::from(app.model.as_str()));
    out.push(Line::from(Span::styled(
        format!("{} · {:.1}s", app.phase, app.phase_duration().as_secs_f64()),
        app.theme.styles.muted,
    )));
    if app.max_steps > 0 {
        out.push(Line::from(Span::styled(
            format!("step {}/{}", app.current_step, app.max_steps),
            app.theme.styles.muted,
        )));
    }
    out.push(Line::from(Span::styled(
        format!("↑{} ↓{}", app.input_tokens, app.output_tokens),
        app.theme.styles.muted,
    )));

    if !app.todos.is_empty() {
        push_gap(&mut out);
        let done = app
            .todos
            .iter()
            .filter(|todo| todo.status == TodoStatus::Completed)
            .count();
        out.push(Line::from(vec![
            Span::styled(
                "TODOS",
                app.theme.styles.accent.add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {done}/{}", app.todos.len()),
                app.theme.styles.muted,
            ),
        ]));
        for todo in app.todos.iter().take(TODO_LIMIT) {
            let (glyph, style) = match todo.status {
                TodoStatus::Pending => ('☐', app.theme.styles.todo_pending),
                TodoStatus::InProgress => ('◐', app.theme.styles.todo_in_progress),
                TodoStatus::Completed => ('☑', app.theme.styles.todo_completed),
            };
            // Una sola línea: los saltos de línea romperían el recorte del rail.
            let content = todo
                .content
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            out.push(Line::from(vec![
                Span::styled(format!("{glyph} "), style),
                Span::raw(content),
            ]));
        }
        more(&mut out, app.todos.len(), TODO_LIMIT, app);
    }

    if !app.changes.is_empty() {
        push_gap(&mut out);
        out.push(title("FILES", app));
        for change in app.changes.iter().take(FILE_LIMIT) {
            out.push(Line::from(vec![
                Span::styled("● ", app.theme.styles.success),
                Span::raw(change.path.as_str()),
                Span::styled(format!("  +{}", change.added), app.theme.styles.success),
                Span::styled(format!("  -{}", change.removed), app.theme.styles.error),
            ]));
        }
        more(&mut out, app.changes.len(), FILE_LIMIT, app);
    }

    if !app.tools.is_empty() {
        push_gap(&mut out);
        out.push(title("TOOLS", app));
        let skip = app.tools.len().saturating_sub(TOOL_LIMIT);
        for tool in app.tools.iter().skip(skip) {
            out.push(glyph_line(
                tool.name.as_str(),
                tool_style(tool.status(), app),
            ));
        }
    }

    let session = app.session_id;
    let jobs = || {
        app.jobs
            .values()
            .filter(move |job| Some(job.owner) == session && job.kind != "subagent")
    };
    let job_total = jobs().count();
    if job_total > 0 {
        push_gap(&mut out);
        out.push(title("JOBS", app));
        for job in jobs().take(JOB_LIMIT) {
            let style = match job.status {
                JobStatus::Running | JobStatus::Stopping => app.theme.styles.tool_running,
                JobStatus::Completed => app.theme.styles.tool_success,
                JobStatus::Failed => app.theme.styles.tool_error,
                JobStatus::Killed => app.theme.styles.tool_canceled,
            };
            out.push(glyph_line(job.label.as_str(), style));
        }
        more(&mut out, job_total, JOB_LIMIT, app);
    }

    let subagents = || {
        app.subagents
            .values()
            .filter(move |subagent| Some(subagent.parent) == session)
    };
    let subagent_total = subagents().count();
    if subagent_total > 0 {
        push_gap(&mut out);
        out.push(title("SUBAGENTS", app));
        for subagent in subagents().take(SUBAGENT_LIMIT) {
            let style = match subagent.status {
                SubagentStatus::Running => app.theme.styles.tool_running,
                SubagentStatus::Completed => app.theme.styles.tool_success,
                SubagentStatus::Failed => app.theme.styles.tool_error,
                SubagentStatus::Killed | SubagentStatus::Interrupted => {
                    app.theme.styles.tool_canceled
                }
            };
            out.push(glyph_line(subagent.label.as_str(), style));
        }
        more(&mut out, subagent_total, SUBAGENT_LIMIT, app);
    }

    out
}

/// Título de sección: acento en negrita.
fn title<'a>(text: &'a str, app: &App) -> Line<'a> {
    Line::from(Span::styled(
        text,
        app.theme.styles.accent.add_modifier(Modifier::BOLD),
    ))
}

/// Línea con un glifo `●` del color indicado y la etiqueta.
fn glyph_line<'a>(label: &'a str, style: Style) -> Line<'a> {
    Line::from(vec![Span::styled("● ", style), Span::raw(label)])
}

/// Resumen `… +n more` cuando la lista tiene más elementos que el límite.
fn more(out: &mut Vec<Line<'_>>, total: usize, limit: usize, app: &App) {
    if total > limit {
        out.push(Line::from(Span::styled(
            format!("… +{} more", total - limit),
            app.theme.styles.muted,
        )));
    }
}

/// Línea en blanco entre secciones, solo si ya hay contenido antes.
fn push_gap(out: &mut Vec<Line<'_>>) {
    if !out.is_empty() {
        out.push(Line::default());
    }
}

fn tool_style(status: ToolStatus, app: &App) -> Style {
    match status {
        ToolStatus::Pending => app.theme.styles.tool_pending,
        ToolStatus::Running => app.theme.styles.tool_running,
        ToolStatus::Success => app.theme.styles.tool_success,
        ToolStatus::Error => app.theme.styles.tool_error,
        ToolStatus::Canceled => app.theme.styles.tool_canceled,
        ToolStatus::AwaitingPermission => app.theme.styles.warning,
    }
}
