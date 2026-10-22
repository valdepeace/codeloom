//! Rail derecho de contexto del look Crush: modelo, ficheros, herramientas,
//! jobs y subagentes de la sesión abierta.

use crate::app::{formato_recuento, AgentStats, App, Focus};
use batten::state::{AgentState, ToolStatus};
use codeloom_core::job::JobStatus;
use codeloom_core::task::TaskState;
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
    // Precio por millón del modelo actual, solo si el catálogo lo conoce.
    if let Some((input, output)) = app.model_prices() {
        out.push(Line::from(Span::styled(
            recortar(&format_prices(input, output), width),
            app.theme.styles.muted,
        )));
    }
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
    // Coste de la sesión bajo los tokens: solo con apuntes, y recortado al rail.
    // Si ningún apunte tiene precio, un «$0+» no informa de nada: se omite la línea.
    if let Some((cost, partial)) = app.estimated_cost().filter(|(cost, _)| *cost > 0.0) {
        out.push(Line::from(Span::styled(
            recortar(&format_cost(cost, partial), width),
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
        // Regla de trazabilidad: bajo el recuento de subagentes en curso se nombra su desglose
        // por estado A2A exacto, partido para caber en el rail. Sin subagentes en curso no hay nada que nombrar.
        if app.active_agents() > 0 {
            let activos: Vec<(TaskState, usize)> = app
                .subagent_state_counts()
                .into_iter()
                .filter(|(estado, _)| !estado.is_terminal())
                .collect();
            let grupo = format!("({})", formato_recuento(&activos));
            for linea in partir_por_palabras(&grupo, width) {
                out.push(Line::from(Span::styled(linea, app.theme.styles.muted)));
            }
        }
        // Un único `Instant::now()` por dibujado: el tiempo transcurrido de todos los hijos.
        let ahora = Instant::now();
        for subagent in subagents().take(SUBAGENT_LIMIT) {
            let estado = agent_state(subagent.status.state);
            // Con el foco en el rail, el hijo elegido con ↑/↓ lleva `›` de acento en vez del glifo.
            // El glifo y el `›` ocupan una columna más el espacio, así que la línea no cambia de anchura.
            let seleccionado =
                app.focus == Focus::Rail && app.rail_selected == Some(subagent.child.0);
            let line = if seleccionado {
                // El `›` sustituye al glifo, de modo que una espera del hijo se conserva
                // tiñéndolo con el color de aviso en lugar del acento.
                let marca = if estado == AgentState::InputRequired {
                    app.theme.styles.warning
                } else {
                    app.theme.styles.accent
                };
                Line::from(vec![
                    Span::styled("› ", marca),
                    Span::raw(subagent.label.as_str()),
                ])
            } else {
                Line::from(vec![
                    Span::styled(
                        format!("{} ", estado.glyph()),
                        estado.style(&app.theme.styles),
                    ),
                    Span::raw(subagent.label.as_str()),
                ])
            };
            out.push(line);
            // Segunda línea tenue con las métricas, solo si el hijo tiene estadísticas.
            if let Some(stats) = app.agent_stats.get(&subagent.child.0) {
                let coste = child_cost_label(app.child_cost(subagent.child));
                let ficheros = app.child_file_count(subagent.child);
                let metricas = metrics_line(stats, coste, ficheros, ahora, width.saturating_sub(2));
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

/// Presentación de un estado de tarea de A2A como estado visual. El `match` es exhaustivo
/// y sin comodín: un estado nuevo del núcleo obliga a decidir su aspecto. Un estado de A2A
/// tiene un único glifo, así que la presentación no agrupa nada aquí.
pub fn agent_state(state: TaskState) -> AgentState {
    match state {
        TaskState::Submitted => AgentState::Submitted,
        TaskState::Working => AgentState::Working,
        TaskState::InputRequired => AgentState::InputRequired,
        TaskState::AuthRequired => AgentState::AuthRequired,
        TaskState::Completed => AgentState::Completed,
        TaskState::Failed => AgentState::Failed,
        TaskState::Canceled => AgentState::Canceled,
        TaskState::Rejected => AgentState::Rejected,
    }
}

/// Parte `texto` en líneas de como mucho `width` columnas, cortando por espacios. Una palabra
/// más ancha que la línea se recorta con `…`, como en el resto del rail.
pub(crate) fn partir_por_palabras(texto: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return Vec::new();
    }
    let mut lineas = Vec::new();
    let mut actual = String::new();
    for palabra in texto.split_whitespace() {
        let palabra = recortar(palabra, width);
        let nueva = if actual.is_empty() {
            palabra.clone()
        } else {
            format!("{actual} {palabra}")
        };
        if anchura(&nueva) <= width {
            actual = nueva;
        } else {
            lineas.push(std::mem::replace(&mut actual, palabra));
        }
    }
    if !actual.is_empty() {
        lineas.push(actual);
    }
    lineas
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
/// de redondeo evitan salidas como «1.0000» o «100.00». Si `partial`, lleva un `+` final:
/// hay apuntes sin precio que no entran en la cifra.
pub fn format_cost(dollars: f64, partial: bool) -> String {
    let base = format_dollars(dollars);
    if partial {
        format!("{base}+")
    } else {
        base
    }
}

/// Cifra de `format_cost` sin el marcador de parcial.
fn format_dollars(dollars: f64) -> String {
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

/// Precio por millón del modelo, p. ej. `$3/$15 /M`. Sin decimales si el precio es entero;
/// si no, hasta dos decimales sin ceros sobrantes.
pub fn format_prices(input: f64, output: f64) -> String {
    format!("{}/{} /M", price_part(input), price_part(output))
}

/// Un precio con el signo `$` delante y sin ceros sobrantes.
fn price_part(price: f64) -> String {
    if price.fract() == 0.0 {
        format!("${price:.0}")
    } else {
        let fixed = format!("{price:.2}");
        format!("${}", fixed.trim_end_matches('0').trim_end_matches('.'))
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

/// Grupo de coste de un hijo para su línea de métricas: `format_cost` sin el espacio tras `≈`
/// (`≈$0.004`, `≈$0.004+`). `None` sin precio o si el coste es nulo, para no mostrar `≈$0`.
fn child_cost_label(cost: Option<(f64, bool)>) -> Option<String> {
    cost.filter(|(dollars, _)| *dollars > 0.0)
        .map(|(dollars, partial)| format_cost(dollars, partial).replacen("≈ ", "≈", 1))
}

/// Línea de métricas de un subagente, p. ej. `2/5 read_file · 1.2k/300 · ≈$0.004 · 7s`.
/// Omite lo que no exista: el paso sin `max_steps`, la herramienta sin curso y
/// los tokens, el coste y el tiempo sin `started_at`. `cost` llega ya formateado y el
/// recuento de ficheros (`files`) solo se muestra si es mayor que cero.
/// Recorta por columnas, terminando en `…`.
fn metrics_line(
    stats: &AgentStats,
    cost: Option<String>,
    files: usize,
    now: Instant,
    width: usize,
) -> String {
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
        grupos.extend(cost);
        if files > 0 {
            grupos.push(format!("{files}f"));
        }
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
    use codeloom_core::session::{SessionId, SubagentDescriptor, UsageRecord};
    use codeloom_core::task::{TaskState, TaskStatus};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    /// Subagente en marcha de la sesión 1 con el id interno `child`.
    fn running(child: i64, label: &str) -> SubagentDescriptor {
        SubagentDescriptor {
            parent: SessionId(1),
            child: SessionId(child),
            job_id: format!("subagent-{child}"),
            label: label.into(),
            status: TaskStatus::new(TaskState::Working),
            artifacts: Vec::new(),
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
        app.subagents.get_mut(&9).unwrap().status = TaskStatus::new(TaskState::InputRequired);

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
            review.contains("? review"),
            "falta el glifo de espera: {review:?}"
        );
        assert!(!tests.contains('?'), "glifo de espera de más: {tests:?}");
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
            metrics_line(&full, None, 0, now, 80),
            "2/5 read_file · 1.2k/300 · 7s"
        );

        let sin_paso = stats(0, 0, Some("read_file"), 1_200, 300, Some(start));
        assert_eq!(
            metrics_line(&sin_paso, None, 0, now, 80),
            "read_file · 1.2k/300 · 7s"
        );

        let sin_herramienta = stats(2, 5, None, 1_200, 300, Some(start));
        assert_eq!(
            metrics_line(&sin_herramienta, None, 0, now, 80),
            "2/5 · 1.2k/300 · 7s"
        );

        let sin_arranque = stats(2, 5, None, 1_200, 300, None);
        assert_eq!(metrics_line(&sin_arranque, None, 0, now, 80), "2/5");

        let terminado = AgentStats {
            finished_at: Some(start + Duration::from_secs(3)),
            ..stats(2, 5, None, 0, 0, Some(start))
        };
        assert_eq!(metrics_line(&terminado, None, 0, now, 80), "2/5 · 0/0 · 3s");
    }

    #[test]
    fn metrics_line_places_cost_between_tokens_and_elapsed() {
        let start = Instant::now();
        let now = start + Duration::from_secs(7);
        let full = stats(2, 5, Some("read_file"), 1_200, 300, Some(start));
        assert_eq!(
            metrics_line(&full, Some("≈$0.004".into()), 0, now, 80),
            "2/5 read_file · 1.2k/300 · ≈$0.004 · 7s"
        );

        // Sin arranque no salen tokens ni tiempo, así que tampoco el grupo de coste.
        let sin_arranque = stats(2, 5, None, 1_200, 300, None);
        assert_eq!(
            metrics_line(&sin_arranque, Some("≈$0.004".into()), 0, now, 80),
            "2/5"
        );
    }

    #[test]
    fn metrics_line_places_file_count_between_cost_and_elapsed() {
        let start = Instant::now();
        let now = start + Duration::from_secs(7);
        let full = stats(2, 5, Some("read_file"), 1_200, 300, Some(start));
        assert_eq!(
            metrics_line(&full, Some("≈$0.004".into()), 3, now, 80),
            "2/5 read_file · 1.2k/300 · ≈$0.004 · 3f · 7s"
        );
        assert_eq!(
            metrics_line(&full, None, 3, now, 80),
            "2/5 read_file · 1.2k/300 · 3f · 7s"
        );

        // Sin arranque no hay grupo de ficheros, igual que con el coste.
        let sin_arranque = stats(2, 5, None, 1_200, 300, None);
        assert_eq!(metrics_line(&sin_arranque, None, 3, now, 80), "2/5");
    }

    #[test]
    fn rail_shows_file_count_for_a_child() {
        let mut app = App::new();
        app.session_id = Some(SessionId(1));
        app.subagents.insert(9, running(9, "review"));
        app.agent_stats
            .insert(9, stats(2, 5, None, 1_200, 300, Some(Instant::now())));
        app.files_by_session.insert(
            9,
            [
                ("src/a.rs".to_string(), (1, 0)),
                ("src/b.rs".to_string(), (2, 2)),
                ("src/c.rs".to_string(), (0, 4)),
            ]
            .into_iter()
            .collect(),
        );

        let rows = rows_of(&app, 40, 24);
        let idx_review = rows
            .iter()
            .position(|row| row.contains("review"))
            .expect("el rail lista el subagente review");
        assert!(
            rows[idx_review + 1].contains("· 3f"),
            "falta el recuento de ficheros bajo review: {:?}",
            rows[idx_review + 1]
        );
    }

    #[test]
    fn rail_omits_file_count_without_files() {
        let mut app = App::new();
        app.session_id = Some(SessionId(1));
        app.subagents.insert(9, running(9, "review"));
        app.agent_stats
            .insert(9, stats(2, 5, None, 1_200, 300, Some(Instant::now())));

        let rows = rows_of(&app, 40, 24);
        let idx_review = rows
            .iter()
            .position(|row| row.contains("review"))
            .expect("el rail lista el subagente review");
        assert!(
            !rows[idx_review + 1].contains('f'),
            "recuento de ficheros sin ficheros: {:?}",
            rows[idx_review + 1]
        );
    }

    #[test]
    fn rail_child_metrics_show_cost_group_between_tokens_and_time() {
        let mut app = priced_rail_app("priced");
        app.session_id = Some(SessionId(1));
        app.subagents.insert(9, running(9, "review"));
        app.agent_stats.insert(
            9,
            stats(
                2,
                5,
                None,
                1_200,
                300,
                Some(Instant::now() - Duration::from_secs(7)),
            ),
        );
        app.usage_records = vec![UsageRecord {
            session: SessionId(9),
            ..record(1_000_000, 0, Some((3.0, 15.0)))
        }];

        let rows = rows_of(&app, 40, 24);
        let fila = rows
            .iter()
            .find(|row| row.contains("1.2k/300 ·"))
            .expect("el rail muestra las métricas del hijo");
        let tokens = fila.find("1.2k/300").expect("tokens");
        let coste = fila.find("≈$3.00").expect("el grupo de coste del hijo");
        let tiempo = fila.find("· 7s").expect("tiempo transcurrido");
        assert!(
            tokens < coste && coste < tiempo,
            "el coste va entre tokens y tiempo: {fila:?}"
        );
    }

    #[test]
    fn rail_child_metrics_omit_cost_group_without_price() {
        let mut app = priced_rail_app("priced");
        app.session_id = Some(SessionId(1));
        app.subagents.insert(9, running(9, "review"));
        app.agent_stats.insert(
            9,
            stats(
                2,
                5,
                None,
                1_200,
                300,
                Some(Instant::now() - Duration::from_secs(7)),
            ),
        );
        app.usage_records = vec![UsageRecord {
            session: SessionId(9),
            ..record(1_000, 1_000, None)
        }];

        let rows = rows_of(&app, 40, 24);
        let fila = rows
            .iter()
            .find(|row| row.contains("1.2k/300 ·"))
            .expect("el rail muestra las métricas del hijo");
        assert!(
            !fila.contains('≈'),
            "sin precio no hay grupo de coste: {fila:?}"
        );
    }

    #[test]
    fn metrics_line_truncates_to_width() {
        let start = Instant::now();
        let now = start + Duration::from_secs(7);
        let herramienta = "t".repeat(50);
        let largo = stats(2, 5, Some(&herramienta), 1_200, 300, Some(start));

        let linea = metrics_line(&largo, None, 0, now, 30);
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
    fn rail_names_the_a2a_states_under_the_running_count() {
        let mut app = App::new();
        app.session_id = Some(SessionId(1));
        app.subagents.insert(9, running(9, "review"));

        let rows = rows_of(&app, 40, 24);
        // Solo los estados que hay, con su cuenta exacta.
        assert!(
            rows.iter().any(|row| row.contains("(working 1)")),
            "falta el desglose por estado bajo el recuento: {rows:?}"
        );
        assert!(
            !rows.iter().any(|row| row.contains("auth-required")),
            "se nombra un estado que no hay: {rows:?}"
        );
    }

    #[test]
    fn grouping_text_wraps_inside_the_rail_width() {
        let texto = "running (submitted, working, input-required, auth-required)";
        let lineas = partir_por_palabras(texto, 20);
        assert!(lineas.len() > 1, "no parte el texto: {lineas:?}");
        assert!(
            lineas.iter().all(|linea| anchura(linea) <= 20),
            "una línea supera el rail: {lineas:?}"
        );
        assert_eq!(lineas.join(" "), texto, "se pierden palabras: {lineas:?}");
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
        assert_eq!(format_cost(0.00005, false), "≈ < $0.0001");
        assert_eq!(format_cost(0.0001, false), "≈ $0.0001");
        assert_eq!(format_cost(0.0123, false), "≈ $0.0123");
        assert_eq!(format_cost(1.234, false), "≈ $1.23");
        assert_eq!(format_cost(123.4, false), "≈ $123");
    }

    #[test]
    fn format_cost_rounding_edges_and_zero() {
        // El redondeo no debe producir «1.0000» ni «100.00».
        assert_eq!(format_cost(0.99996, false), "≈ $1.00");
        assert_eq!(format_cost(99.996, false), "≈ $100");
        assert_eq!(format_cost(0.0, false), "≈ $0");
    }

    #[test]
    fn format_cost_partial_adds_plus() {
        assert_eq!(format_cost(0.0123, true), "≈ $0.0123+");
        assert_eq!(format_cost(0.0, true), "≈ $0+");
    }

    #[test]
    fn format_prices_shapes() {
        assert_eq!(format_prices(3.0, 15.0), "$3/$15 /M");
        assert_eq!(format_prices(0.3, 2.5), "$0.3/$2.5 /M");
        assert_eq!(format_prices(0.25, 1.0), "$0.25/$1 /M");
    }

    /// Apunte con precios `(entrada, salida)` por millón, o sin precio si `prices` es `None`.
    fn record(input: u64, output: u64, prices: Option<(f64, f64)>) -> UsageRecord {
        UsageRecord {
            session: SessionId(1),
            model: "priced".into(),
            input_tokens: input,
            output_tokens: output,
            input_price: prices.map(|(price_in, _)| price_in),
            output_price: prices.map(|(_, price_out)| price_out),
            created_at_ms: 1,
        }
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
    fn rail_shows_session_total_and_model_prices() {
        let mut app = priced_rail_app("priced");
        app.usage_records = vec![
            record(1_000_000, 0, Some((3.0, 15.0))),
            record(0, 100_000, Some((3.0, 15.0))),
        ];
        let rows = rows_of(&app, 40, 24);
        // Entrada: 3,00. Salida: 100.000 a 15 $/M = 1,50. Total: 4,50.
        assert!(
            rows.iter().any(|row| row.contains("≈ $4.50")),
            "el rail muestra el total de la sesión: {rows:?}"
        );
        assert!(
            rows.iter().any(|row| row.contains("$3/$15 /M")),
            "el rail muestra el precio por millón del modelo: {rows:?}"
        );
    }

    #[test]
    fn rail_hides_cost_line_when_no_record_has_a_price() {
        use codeloom_core::session::UsageRecord;
        let mut app = App::new();
        app.session_id = Some(SessionId(1));
        app.usage_records.push(UsageRecord {
            session: SessionId(1),
            model: "m".into(),
            input_tokens: 10,
            output_tokens: 5,
            input_price: None,
            output_price: None,
            created_at_ms: 0,
        });
        let rows = rows_of(&app, 40, 24);
        assert!(
            !rows.iter().any(|row| row.contains('≈')),
            "sale un coste sin precio alguno: {rows:?}"
        );
    }

    #[test]
    fn rail_hides_prices_line_without_catalog() {
        let mut app = App::new();
        app.usage_records = vec![record(1_000, 1_000, None)];
        let rows = rows_of(&app, 40, 24);
        assert!(
            !rows.iter().any(|row| row.contains("/M")),
            "sin catálogo no hay línea de precios: {rows:?}"
        );
    }

    #[test]
    fn rail_shows_cost_line_when_priced() {
        let mut app = priced_rail_app("priced");
        app.input_tokens = 1_000_000;
        app.usage_records = vec![record(1_000_000, 0, Some((3.0, 15.0)))];

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
        app.usage_records = vec![record(1_000_000_000_000_000, 0, Some((3.0, 15.0)))];

        let rows = rows_of(&app, 12, 24);
        let fila = rows
            .iter()
            .find(|row| row.contains('≈'))
            .expect("la línea de coste aparece también en un rail estrecho");
        assert_eq!(fila.chars().count(), 12, "la fila ocupa el ancho del rail");
        assert!(fila.contains('…'), "la línea no se recorta: {fila:?}");
    }

    #[test]
    fn rail_shows_input_required_glyph_for_waiting_running_child() {
        let mut app = App::new();
        app.session_id = Some(SessionId(1));
        app.subagents.insert(9, running(9, "review"));
        app.subagents.insert(10, running(10, "tests"));
        app.subagents.get_mut(&9).unwrap().status = TaskStatus::new(TaskState::InputRequired);

        let rows = rows_of(&app, 40, 24);
        assert!(
            rows.iter().any(|row| row.contains("? review")),
            "falta el glifo input-required del hijo en espera: {rows:?}"
        );
        assert!(
            rows.iter().any(|row| row.contains("● tests")),
            "el hijo sin espera debe pintarse como working: {rows:?}"
        );
    }

    #[test]
    fn rail_shows_failed_glyph_for_a_failed_child() {
        let mut app = App::new();
        app.session_id = Some(SessionId(1));
        app.subagents.insert(
            9,
            SubagentDescriptor {
                status: TaskStatus::new(TaskState::Failed),
                ..running(9, "review")
            },
        );

        let rows = rows_of(&app, 40, 24);
        assert!(
            rows.iter().any(|row| row.contains("✗ review")),
            "un hijo sin terminar debe verse como failed: {rows:?}"
        );
    }

    #[test]
    fn rail_shows_completed_glyph_for_finished_child() {
        let mut app = App::new();
        app.session_id = Some(SessionId(1));
        app.subagents.insert(
            9,
            SubagentDescriptor {
                status: TaskStatus::new(TaskState::Completed),
                ..running(9, "review")
            },
        );

        let rows = rows_of(&app, 40, 24);
        let fila = rows
            .iter()
            .find(|row| row.contains("review"))
            .expect("el rail lista review");
        assert!(
            fila.contains("✓ review"),
            "un hijo terminado debe verse como completed: {fila:?}"
        );
        assert!(
            !fila.contains('?'),
            "espera residual en un hijo terminado: {fila:?}"
        );
    }

    #[test]
    fn agent_state_covers_all_eight_states() {
        // Submitted, AuthRequired y Rejected no tienen productor hoy: se construyen a mano.
        let pares = [
            (TaskState::Submitted, AgentState::Submitted),
            (TaskState::Working, AgentState::Working),
            (TaskState::InputRequired, AgentState::InputRequired),
            (TaskState::AuthRequired, AgentState::AuthRequired),
            (TaskState::Completed, AgentState::Completed),
            (TaskState::Failed, AgentState::Failed),
            (TaskState::Canceled, AgentState::Canceled),
            (TaskState::Rejected, AgentState::Rejected),
        ];
        for (tarea, esperado) in pares {
            assert_eq!(agent_state(tarea), esperado, "proyección de {tarea:?}");
        }
    }

    #[test]
    fn rail_selected_child_keeps_arrow_marker() {
        let mut app = App::new();
        app.session_id = Some(SessionId(1));
        app.subagents.insert(9, running(9, "review"));
        app.subagents.insert(10, running(10, "tests"));
        app.rail_selected = Some(9);
        app.subagents.get_mut(&9).unwrap().status = TaskStatus::new(TaskState::InputRequired);
        app.focus = crate::app::Focus::Rail;

        let rows = rows_of(&app, 40, 24);
        assert!(
            rows.iter().any(|row| row.contains("› review")),
            "el hijo elegido con foco en el rail debe llevar ›: {rows:?}"
        );
        assert!(
            !rows.iter().any(|row| row.contains("? review")),
            "el › sustituye al glifo del hijo elegido: {rows:?}"
        );
        assert!(
            rows.iter().any(|row| row.contains("● tests")),
            "el hijo no elegido conserva su glifo: {rows:?}"
        );
    }

    #[test]
    fn selected_child_marker_is_tinted_when_waiting_for_input() {
        let mut app = App::new();
        app.session_id = Some(SessionId(1));
        app.subagents.insert(9, running(9, "review"));
        app.subagents.insert(10, running(10, "tests"));
        app.focus = crate::app::Focus::Rail;
        app.rail_selected = Some(9);
        app.subagents.get_mut(&9).unwrap().status = TaskStatus::new(TaskState::InputRequired);
        let marca = |app: &App, etiqueta: &str| {
            rail_lines(app, 40)
                .into_iter()
                .find(|line| line.spans.iter().any(|s| s.content == etiqueta))
                .and_then(|line| line.spans.first().map(|s| s.style))
                .expect("la línea del hijo existe")
        };
        assert_eq!(marca(&app, "review"), app.theme.styles.warning);

        // Sin espera, el marcador vuelve al acento.
        app.subagents.get_mut(&9).unwrap().status = TaskStatus::new(TaskState::Working);
        assert_eq!(marca(&app, "review"), app.theme.styles.accent);
    }

    #[test]
    fn rail_derives_input_and_auth_from_the_task_state() {
        let mut app = App::new();
        app.session_id = Some(SessionId(1));
        app.subagents.insert(
            9,
            SubagentDescriptor {
                status: TaskStatus::new(TaskState::InputRequired),
                ..running(9, "review")
            },
        );
        app.subagents.insert(
            10,
            SubagentDescriptor {
                status: TaskStatus::new(TaskState::AuthRequired),
                ..running(10, "tests")
            },
        );

        let rows = rows_of(&app, 40, 24);
        let glifo_espera = agent_state(TaskState::InputRequired).glyph();
        let glifo_permiso = agent_state(TaskState::AuthRequired).glyph();
        assert!(
            rows.iter()
                .any(|row| row.contains(&format!("{glifo_espera} review"))),
            "el glifo de input-required sale del estado de la tarea: {rows:?}"
        );
        assert!(
            rows.iter()
                .any(|row| row.contains(&format!("{glifo_permiso} tests"))),
            "el glifo de auth-required sale del estado de la tarea: {rows:?}"
        );
    }
}
