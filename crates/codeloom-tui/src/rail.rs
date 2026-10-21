//! Rail derecho de contexto del look Crush: modelo, ficheros, herramientas,
//! jobs y subagentes de la sesión abierta.

use crate::app::{AgentStats, App, Focus};
use batten::state::ToolStatus;
use codeloom_core::job::JobStatus;
use codeloom_core::session::SubagentStatus;
use codeloom_core::todo::TodoStatus;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use std::time::{Duration, Instant};

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
    frame.render_widget(
        Paragraph::new(rail_lines(app, usize::from(inner.width))),
        inner,
    );
}

/// Construye las líneas del rail. `width` es la anchura útil del área interior
/// y solo se usa para recortar las métricas de los subagentes. Cada lista se
/// limita antes de construir sus líneas, así que el coste no crece con el
/// número de elementos.
fn rail_lines(app: &App, width: usize) -> Vec<Line<'_>> {
    let mut out: Vec<Line<'_>> = Vec::new();

    out.push(title("CODELOOM", app));
    out.push(Line::from(Span::styled(
        app.cwd.as_str(),
        app.theme.styles.muted,
    )));
    // Dentro de un subagente, recordatorio de cómo volver al padre. Se recorta al rail.
    if !app.parent_stack.is_empty() {
        out.push(Line::from(Span::styled(
            recortar("↩ /parent · Alt+←", width),
            app.theme.styles.muted,
        )));
    }

    push_gap(&mut out);
    out.push(title("MODEL", app));
    out.push(Line::from(app.model.as_str()));
    // El nivel se lee de la copia guardada, la misma que `app.model`.
    if let Some(level) = app
        .settings_baseline
        .as_ref()
        .and_then(|settings| settings.effort.as_deref())
    {
        out.push(Line::from(Span::styled(
            format!("effort {level}"),
            app.theme.styles.muted,
        )));
    }
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
    // Estimación de coste bajo los tokens: solo con precio y tokens, y recortada al rail.
    if let Some(cost) = app.estimated_cost() {
        out.push(Line::from(Span::styled(
            recortar(&format_cost(cost), width),
            app.theme.styles.muted,
        )));
    }

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
        out.push(subagents_title(app));
        // Un único `Instant::now()` por dibujado: el tiempo transcurrido de todos los hijos.
        let ahora = Instant::now();
        for subagent in subagents().take(SUBAGENT_LIMIT) {
            let style = match subagent.status {
                SubagentStatus::Running => app.theme.styles.tool_running,
                SubagentStatus::Completed => app.theme.styles.tool_success,
                SubagentStatus::Failed => app.theme.styles.tool_error,
                SubagentStatus::Killed | SubagentStatus::Interrupted => {
                    app.theme.styles.tool_canceled
                }
            };
            // Con el foco en el rail, el hijo elegido con ↑/↓ lleva `›` de acento en vez de `●`.
            // Ambos glifos ocupan dos columnas, así que la línea no cambia de anchura.
            let seleccionado =
                app.focus == Focus::Rail && app.rail_selected == Some(subagent.child.0);
            let mut line = if seleccionado {
                Line::from(vec![
                    Span::styled("› ", app.theme.styles.accent),
                    Span::raw(subagent.label.as_str()),
                ])
            } else {
                glyph_line(subagent.label.as_str(), style)
            };
            // Marca de espera: un «?» de acento tras la etiqueta. Solo una consulta al conjunto.
            if app.awaiting_input.contains(&subagent.child.0) {
                line.spans
                    .push(Span::styled(" ?", app.theme.styles.warning));
            }
            out.push(line);
            // Segunda línea tenue con las métricas, solo si el hijo tiene estadísticas.
            if let Some(stats) = app.agent_stats.get(&subagent.child.0) {
                let metricas = metrics_line(stats, ahora, width.saturating_sub(2));
                if !metricas.is_empty() {
                    out.push(Line::from(Span::styled(
                        format!("  {metricas}"),
                        app.theme.styles.muted,
                    )));
                }
            }
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

/// Título de SUBAGENTS. Con actividad o tokens acumulados añade el número de
/// agentes en marcha y los totales de la jerarquía: `SUBAGENTS · 2 · 2k/400`.
fn subagents_title(app: &App) -> Line<'static> {
    let (input, output) = app.hierarchy_tokens();
    let activos = app.active_agents();
    let texto = if (input, output) != (0, 0) || activos > 0 {
        format!(
            "SUBAGENTS · {activos} · {}/{}",
            format_tokens(input),
            format_tokens(output)
        )
    } else {
        "SUBAGENTS".to_owned()
    };
    Line::from(Span::styled(
        texto,
        app.theme.styles.accent.add_modifier(Modifier::BOLD),
    ))
}

/// Cifra compacta de tokens: `999`, `1.2k`, `15k`, `1.5M`, `2G`. El redondeo es
/// hacia abajo (`1_999` → `1.9k`). La decimal solo aparece con una cifra entera
/// y es distinta de cero; a partir de diez se muestra solo la parte entera.
pub fn format_tokens(n: u64) -> String {
    if n < 1_000 {
        return n.to_string();
    }
    let (unidad, divisor) = if n < 1_000_000 {
        ('k', 1_000)
    } else if n < 1_000_000_000 {
        ('M', 1_000_000)
    } else {
        ('G', 1_000_000_000)
    };
    let entero = n / divisor;
    if entero < 10 {
        let decima = (n % divisor) / (divisor / 10);
        if decima > 0 {
            return format!("{entero}.{decima}{unidad}");
        }
    }
    format!("{entero}{unidad}")
}

/// Coste estimado en dólares, siempre con el prefijo `≈` de estimación: `≈ $0.0123` por
/// debajo de uno (cuatro decimales), `≈ $1.23` hasta cien y `≈ $123` desde ahí. Un valor
/// positivo pero menor que una diezmilésima se muestra como `≈ < $0.0001`. Los umbrales
/// de redondeo evitan salidas como «1.0000» o «100.00».
pub fn format_cost(dollars: f64) -> String {
    if dollars <= 0.0 {
        return "≈ $0".to_owned();
    }
    if dollars < 0.0001 {
        return "≈ < $0.0001".to_owned();
    }
    if dollars < 0.99995 {
        format!("≈ ${dollars:.4}")
    } else if dollars < 99.995 {
        format!("≈ ${dollars:.2}")
    } else {
        format!("≈ ${dollars:.0}")
    }
}

/// Tiempo transcurrido compacto: `7s`, `1m05s` (menos de una hora) o `1h01m`.
pub fn format_elapsed(d: Duration) -> String {
    let total = d.as_secs();
    if total < 60 {
        format!("{total}s")
    } else if total < 3_600 {
        format!("{}m{:02}s", total / 60, total % 60)
    } else {
        format!("{}h{:02}m", total / 3_600, (total % 3_600) / 60)
    }
}

/// Línea de métricas de un subagente, p. ej. `2/5 read_file · 1.2k/300 · 7s`.
/// Omite lo que no exista: el paso sin `max_steps`, la herramienta sin curso y
/// los tokens y el tiempo sin `started_at`. Recorta por columnas, terminando en `…`.
fn metrics_line(stats: &AgentStats, now: Instant, width: usize) -> String {
    let mut grupos: Vec<String> = Vec::new();

    let mut paso = Vec::new();
    if stats.max_steps > 0 {
        paso.push(format!("{}/{}", stats.step, stats.max_steps));
    }
    if let Some(tool) = &stats.tool {
        paso.push(tool.clone());
    }
    if !paso.is_empty() {
        grupos.push(paso.join(" "));
    }

    if let Some(inicio) = stats.started_at {
        let fin = stats.finished_at.unwrap_or(now);
        grupos.push(format!(
            "{}/{}",
            format_tokens(stats.input_tokens),
            format_tokens(stats.output_tokens)
        ));
        grupos.push(format_elapsed(fin.saturating_duration_since(inicio)));
    }

    recortar(&grupos.join(" · "), width)
}

/// Anchura en columnas de `texto`, medida con la misma lógica que ratatui.
fn anchura(texto: &str) -> usize {
    Span::raw(texto).width()
}

/// Recorta `texto` para que ocupe como mucho `width` columnas. Si recorta, el
/// último carácter visible es `…`, que cuenta como una columna.
fn recortar(texto: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if anchura(texto) <= width {
        return texto.to_owned();
    }
    let mut out = String::new();
    let mut usado = 0;
    let mut buf = [0u8; 4];
    for c in texto.chars() {
        let ancho = anchura(c.encode_utf8(&mut buf));
        if usado + ancho > width - 1 {
            break;
        }
        out.push(c);
        usado += ancho;
    }
    out.push('…');
    out
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

#[cfg(test)]
mod tests {
    use super::*;
    use codeloom_core::session::{SessionId, SubagentDescriptor};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    /// Subagente en marcha de la sesión 1 con el id interno `child`.
    fn running(child: i64, label: &str) -> SubagentDescriptor {
        SubagentDescriptor {
            parent: SessionId(1),
            child: SessionId(child),
            job_id: format!("subagent-{child}"),
            label: label.into(),
            status: SubagentStatus::Running,
            result: None,
            notice_delivered: false,
        }
    }

    /// Filas del búfer de `width` columnas, una cadena por fila.
    fn rows_of(app: &App, width: u16, height: u16) -> Vec<String> {
        let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
        term.draw(|f| draw_context_rail(f, f.area(), app)).unwrap();
        let buffer = term.backend().buffer().clone();
        buffer
            .content
            .chunks(usize::from(width))
            .map(|row| row.iter().map(|cell| cell.symbol()).collect())
            .collect()
    }

    #[test]
    fn rail_marks_subagents_waiting_for_input() {
        let mut app = App::new();
        app.session_id = Some(SessionId(1));
        app.subagents.insert(9, running(9, "review"));
        app.subagents.insert(10, running(10, "tests"));
        app.awaiting_input.insert(9);

        let rows = rows_of(&app, 40, 20);
        let review = rows
            .iter()
            .find(|row| row.contains("review"))
            .expect("el rail lista el subagente review");
        let tests = rows
            .iter()
            .find(|row| row.contains("tests"))
            .expect("el rail lista el subagente tests");
        assert!(
            review.contains(" ?"),
            "falta la marca de espera: {review:?}"
        );
        assert!(!tests.contains(" ?"), "marca de espera de más: {tests:?}");
    }

    /// Estadísticas de un hijo con paso, herramienta y tokens; `started` fija el arranque.
    fn stats(
        step: usize,
        max_steps: usize,
        tool: Option<&str>,
        input: u64,
        output: u64,
        started: Option<Instant>,
    ) -> AgentStats {
        AgentStats {
            step,
            max_steps,
            tool: tool.map(str::to_owned),
            input_tokens: input,
            output_tokens: output,
            started_at: started,
            finished_at: None,
        }
    }

    #[test]
    fn format_tokens_scales() {
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(999), "999");
        assert_eq!(format_tokens(1_200), "1.2k");
        assert_eq!(format_tokens(1_999), "1.9k", "el redondeo es hacia abajo");
        assert_eq!(format_tokens(9_999), "9.9k");
        assert_eq!(format_tokens(15_000), "15k");
        assert_eq!(
            format_tokens(999_999),
            "999k",
            "el límite de k no se desborda"
        );
        assert_eq!(format_tokens(1_000_000), "1M");
        assert_eq!(format_tokens(1_500_000), "1.5M");
        assert_eq!(format_tokens(10_000_000), "10M");
        assert_eq!(format_tokens(2_000_000_000), "2G");
    }

    #[test]
    fn format_elapsed_scales() {
        assert_eq!(format_elapsed(Duration::from_secs(0)), "0s");
        assert_eq!(format_elapsed(Duration::from_secs(7)), "7s");
        assert_eq!(format_elapsed(Duration::from_secs(59)), "59s");
        assert_eq!(format_elapsed(Duration::from_secs(65)), "1m05s");
        assert_eq!(format_elapsed(Duration::from_secs(3_599)), "59m59s");
        assert_eq!(format_elapsed(Duration::from_secs(3_600)), "1h00m");
        assert_eq!(format_elapsed(Duration::from_secs(3_700)), "1h01m");
    }

    #[test]
    fn metrics_line_omits_missing_parts() {
        let start = Instant::now();
        let now = start + Duration::from_secs(7);

        let full = stats(2, 5, Some("read_file"), 1_200, 300, Some(start));
        assert_eq!(
            metrics_line(&full, now, 80),
            "2/5 read_file · 1.2k/300 · 7s"
        );

        let sin_paso = stats(0, 0, Some("read_file"), 1_200, 300, Some(start));
        assert_eq!(
            metrics_line(&sin_paso, now, 80),
            "read_file · 1.2k/300 · 7s"
        );

        let sin_herramienta = stats(2, 5, None, 1_200, 300, Some(start));
        assert_eq!(
            metrics_line(&sin_herramienta, now, 80),
            "2/5 · 1.2k/300 · 7s"
        );

        let sin_arranque = stats(2, 5, None, 1_200, 300, None);
        assert_eq!(metrics_line(&sin_arranque, now, 80), "2/5");

        let terminado = AgentStats {
            finished_at: Some(start + Duration::from_secs(3)),
            ..stats(2, 5, None, 0, 0, Some(start))
        };
        assert_eq!(metrics_line(&terminado, now, 80), "2/5 · 0/0 · 3s");
    }

    #[test]
    fn metrics_line_truncates_to_width() {
        let start = Instant::now();
        let now = start + Duration::from_secs(7);
        let herramienta = "t".repeat(50);
        let largo = stats(2, 5, Some(&herramienta), 1_200, 300, Some(start));

        let linea = metrics_line(&largo, now, 30);
        assert!(linea.chars().count() <= 30, "supera la anchura: {linea:?}");
        assert!(linea.ends_with('…'), "no acaba en elipsis: {linea:?}");
    }

    #[test]
    fn rail_shows_metrics_second_line_for_a_subagent_with_stats() {
        let mut app = App::new();
        app.session_id = Some(SessionId(1));
        app.subagents.insert(9, running(9, "review"));
        app.subagents.insert(10, running(10, "tests"));
        app.agent_stats.insert(
            9,
            stats(2, 5, Some("read_file"), 1_200, 300, Some(Instant::now())),
        );

        let rows = rows_of(&app, 40, 24);
        let idx_review = rows
            .iter()
            .position(|row| row.contains("review"))
            .expect("el rail lista el subagente review");
        let idx_tests = rows
            .iter()
            .position(|row| row.contains("tests"))
            .expect("el rail lista el subagente tests");
        assert!(
            rows[idx_review + 1].contains("2/5 read_file"),
            "falta la línea de métricas bajo review: {:?}",
            rows[idx_review + 1]
        );
        assert!(
            !rows[idx_tests + 1].contains('·'),
            "métricas bajo un subagente sin stats: {:?}",
            rows[idx_tests + 1]
        );
    }

    #[test]
    fn rail_title_shows_hierarchy_totals() {
        let mut app = App::new();
        app.session_id = Some(SessionId(1));
        app.subagents.insert(9, running(9, "review"));
        app.subagents.insert(10, running(10, "tests"));
        let now = Some(Instant::now());
        app.agent_stats
            .insert(9, stats(1, 0, None, 1_200, 300, now));
        app.agent_stats.insert(10, stats(1, 0, None, 800, 100, now));

        let rows = rows_of(&app, 40, 24);
        assert!(
            rows.iter()
                .any(|row| row.contains("SUBAGENTS · 2 · 2k/400")),
            "el título no muestra los totales: {rows:?}"
        );
    }

    #[test]
    fn rail_without_subagents_is_unchanged() {
        let mut app = App::new();
        app.session_id = Some(SessionId(1));

        let rows = rows_of(&app, 40, 24);
        assert!(
            !rows.iter().any(|row| row.contains("SUBAGENTS")),
            "aparece la sección SUBAGENTS sin subagentes: {rows:?}"
        );
    }

    /// Columna (en caracteres) donde empieza `texto` dentro de `fila`.
    fn columna(fila: &str, texto: &str) -> usize {
        let byte = fila.find(texto).expect("el texto debe estar en la fila");
        fila[..byte].chars().count()
    }

    #[test]
    fn selected_subagent_shows_marker_only_when_rail_focused() {
        let mut app = App::new();
        app.session_id = Some(SessionId(1));
        app.subagents.insert(9, running(9, "review"));
        app.subagents.insert(10, running(10, "tests"));
        app.rail_selected = Some(9);

        app.focus = crate::app::Focus::Composer;
        let sin_foco = rows_of(&app, 40, 24);
        let fila_sin_foco = sin_foco
            .iter()
            .find(|row| row.contains("review"))
            .expect("el rail lista review");
        assert!(
            !fila_sin_foco.contains('›'),
            "marcador de selección sin foco en el rail: {fila_sin_foco:?}"
        );
        assert!(fila_sin_foco.contains("● review"));

        app.focus = crate::app::Focus::Rail;
        let con_foco = rows_of(&app, 40, 24);
        let fila_review = con_foco
            .iter()
            .find(|row| row.contains("review"))
            .expect("el rail lista review");
        let fila_tests = con_foco
            .iter()
            .find(|row| row.contains("tests"))
            .expect("el rail lista tests");
        assert!(
            fila_review.contains("› review"),
            "falta el marcador con foco en el rail: {fila_review:?}"
        );
        assert!(
            fila_tests.contains("● tests") && !fila_tests.contains('›'),
            "marcador en un subagente no seleccionado: {fila_tests:?}"
        );
        assert_eq!(
            columna(fila_review, "review"),
            columna(fila_sin_foco, "review"),
            "el marcador cambia la anchura de la línea"
        );
    }

    #[test]
    fn rail_shows_return_hint_inside_a_child() {
        let mut app = App::new();
        app.session_id = Some(SessionId(9));
        app.cwd = "/proj".into();
        app.parent_stack.push(SessionId(1));

        let rows = rows_of(&app, 40, 24);
        let idx = rows
            .iter()
            .position(|row| row.contains("CODELOOM"))
            .expect("el rail muestra el título CODELOOM");
        assert!(rows[idx + 1].contains("/proj"), "falta el cwd: {rows:?}");
        assert!(
            rows[idx + 2].contains("↩ /parent · Alt+←"),
            "falta la línea de retorno bajo el cwd: {:?}",
            rows[idx + 2]
        );
    }

    #[test]
    fn rail_has_no_return_hint_at_top_level() {
        let mut app = App::new();
        app.session_id = Some(SessionId(1));
        app.cwd = "/proj".into();

        let rows = rows_of(&app, 40, 24);
        assert!(
            !rows.iter().any(|row| row.contains('↩')),
            "línea de retorno en el nivel superior: {rows:?}"
        );
    }

    #[test]
    fn return_hint_is_clipped_to_rail_width() {
        let mut app = App::new();
        app.session_id = Some(SessionId(9));
        app.cwd = "/proj".into();
        app.parent_stack.push(SessionId(1));

        // Con un borde izquierdo, la anchura útil del rail es 11 columnas.
        let rows = rows_of(&app, 12, 24);
        let fila = rows
            .iter()
            .find(|row| row.contains('↩'))
            .expect("la línea de retorno aparece también en un rail estrecho");
        assert!(fila.contains('…'), "la línea no se recorta: {fila:?}");
        assert!(
            !fila.contains("Alt"),
            "la línea desborda la anchura del rail: {fila:?}"
        );
    }

    /// Ajustes guardados con el modelo `claude-think` y el esfuerzo `effort`.
    fn saved_settings(effort: Option<&str>) -> crate::app::ProviderSettings {
        crate::app::ProviderSettings {
            kind: codeloom_core::config::ProviderKind::Anthropic,
            model: "claude-think".into(),
            base_url: "https://api.anthropic.com/v1".into(),
            api_key_env: "ANTHROPIC_API_KEY".into(),
            effort: effort.map(String::from),
        }
    }

    #[test]
    fn rail_shows_effort_when_set() {
        let mut app = App::new();
        app.set_provider_settings(saved_settings(Some("high")));
        let rows = rows_of(&app, 40, 20);
        assert!(
            rows.iter().any(|row| row.contains("effort high")),
            "el rail muestra el esfuerzo bajo el modelo: {rows:?}"
        );

        app.set_provider_settings(saved_settings(None));
        let rows = rows_of(&app, 40, 20);
        assert!(
            !rows.iter().any(|row| row.contains("effort")),
            "sin esfuerzo no hay línea en el rail: {rows:?}"
        );
    }

    #[test]
    fn format_cost_ranges() {
        assert_eq!(format_cost(0.00005), "≈ < $0.0001");
        assert_eq!(format_cost(0.0001), "≈ $0.0001");
        assert_eq!(format_cost(0.0123), "≈ $0.0123");
        assert_eq!(format_cost(1.234), "≈ $1.23");
        assert_eq!(format_cost(123.4), "≈ $123");
    }

    #[test]
    fn format_cost_rounding_edges_and_zero() {
        // El redondeo no debe producir «1.0000» ni «100.00».
        assert_eq!(format_cost(0.99996), "≈ $1.00");
        assert_eq!(format_cost(99.996), "≈ $100");
        assert_eq!(format_cost(0.0), "≈ $0");
    }

    /// Ajustes de `acme` con el modelo dado, con el catálogo de precios cargado.
    fn priced_rail_app(model: &str) -> App {
        let mut app = App::new();
        app.set_catalog(crate::app::priced_sample_catalog());
        app.set_provider_settings(crate::app::ProviderSettings {
            kind: codeloom_core::config::ProviderKind::OpenAiCompat,
            model: model.into(),
            base_url: "https://acme.example/v1".into(),
            api_key_env: "ACME_KEY".into(),
            effort: None,
        });
        app
    }

    #[test]
    fn rail_shows_cost_line_when_priced() {
        let mut app = priced_rail_app("priced");
        app.input_tokens = 1_000_000;

        let rows = rows_of(&app, 40, 24);
        let idx = rows
            .iter()
            .position(|row| row.contains("↑1000000"))
            .expect("el rail muestra la línea de tokens");
        assert!(
            rows[idx + 1].contains("≈ $3.00"),
            "falta el coste justo bajo los tokens: {:?}",
            rows[idx + 1]
        );
    }

    #[test]
    fn rail_has_no_cost_line_without_prices() {
        let mut app = App::new();
        app.input_tokens = 1_000_000;
        let rows = rows_of(&app, 40, 24);
        assert!(
            !rows.iter().any(|row| row.contains('≈')),
            "línea de coste sin precio: {rows:?}"
        );

        // Con catálogo, pero el modelo no tiene precio completo: tampoco.
        let mut app = priced_rail_app("plain");
        app.input_tokens = 1_000_000;
        let rows = rows_of(&app, 40, 24);
        assert!(
            !rows.iter().any(|row| row.contains('≈')),
            "línea de coste sin precio del modelo: {rows:?}"
        );
    }

    #[test]
    fn rail_clips_huge_cost_to_width() {
        // Tres mil millones de dólares: la cifra no debe desbordar un rail de 11 columnas.
        let mut app = priced_rail_app("priced");
        app.input_tokens = 1_000_000_000_000_000;

        let rows = rows_of(&app, 12, 24);
        let fila = rows
            .iter()
            .find(|row| row.contains('≈'))
            .expect("la línea de coste aparece también en un rail estrecho");
        assert_eq!(fila.chars().count(), 12, "la fila ocupa el ancho del rail");
        assert!(fila.contains('…'), "la línea no se recorta: {fila:?}");
    }
}
