use crate::app::{App, Focus, SettingsPicker};
use crate::rail;
use batten::widgets::context_meter::{self, MeterStyle};
use batten::widgets::{ChromeStyle, LayoutStyle};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

fn job_counts(app: &App) -> (usize, usize, usize, usize) {
    let (running, finished) = app
        .jobs
        .values()
        .filter(|job| Some(job.owner) == app.session_id && job.kind != "subagent")
        .fold((0, 0), |(running, finished), job| {
            let active = matches!(
                job.status,
                codeloom_core::job::JobStatus::Running | codeloom_core::job::JobStatus::Stopping
            );
            (
                running + usize::from(active),
                finished + usize::from(!active),
            )
        });
    let (subagents_running, subagents_finished) = app
        .subagents
        .values()
        .filter(|subagent| Some(subagent.parent) == app.session_id)
        .fold((0, 0), |(running, finished), subagent| {
            if subagent.status.state.is_terminal() {
                (running, finished + 1)
            } else {
                (running + 1, finished)
            }
        });
    (running, finished, subagents_running, subagents_finished)
}

/// Altura de la cabecera de espacio de trabajo: dos líneas, o tres si hay subagentes que nombrar.
fn workspace_header_height(app: &App) -> u16 {
    if app.subagent_state_counts().is_empty() {
        2
    } else {
        3
    }
}

fn draw_workspace_header(frame: &mut Frame, area: Rect, app: &App) {
    let (state, tone) = if app.status == "working" {
        ("WORKING", batten::state::Tone::Success)
    } else if app.status.starts_with("error:") {
        ("ERROR", batten::state::Tone::Error)
    } else {
        ("READY", batten::state::Tone::Neutral)
    };
    let mut text = vec![
        Span::styled(
            " CODELOOM / WORKSPACE  ",
            app.theme.styles.heading.add_modifier(Modifier::BOLD),
        ),
        batten::widgets::chip(state, tone, &app.theme),
        Span::styled(format!("  {}", app.model), app.theme.styles.muted),
    ];
    if app.queued > 0 {
        text.push(Span::styled(
            format!("  ·  {} queued", app.queued),
            app.theme.styles.muted,
        ));
    }
    if app.queue_paused {
        text.push(Span::styled("  ·  queue paused", app.theme.styles.warning));
    }
    let (running, finished, subagents, subagents_finished) = job_counts(app);
    if running > 0 || finished > 0 || subagents > 0 || subagents_finished > 0 {
        // El desglose de los subagentes va en su propia línea (ver más abajo): aquí solo los jobs
        // y el total de «finished», que incluye los terminados de los subagentes.
        text.push(Span::styled(
            format!(
                "  ·  {running} jobs · {} finished",
                finished + subagents_finished
            ),
            app.theme.styles.muted,
        ));
    }
    let mut lineas = vec![Line::from(text)];
    // Regla de trazabilidad: el resumen de subagentes nombra sus estados de A2A exactos. Va en
    // una línea propia, así no se recorta a 100 columnas; sin subagentes no existe esa línea.
    if !app.subagent_state_counts().is_empty() {
        lineas.push(Line::from(Span::styled(
            format!("  {}{}", app.running_summary(), app.finished_summary()),
            app.theme.styles.muted,
        )));
    }
    frame.render_widget(
        Paragraph::new({
            lineas.push(Line::from(Span::styled(
                "   conversation · activity · context · changed files",
                app.theme.styles.muted,
            )));
            lineas
        }),
        area,
    );
}

/// Sufijo de la línea de estado mientras corre el resumen anticipado: `  · resumiendo… N s`.
/// Vacío si no corre. Va tras el estado, así que si no cabe se recorta como el resto de la línea.
fn background_suffix(app: &App) -> String {
    app.background_summary_label()
        .map_or_else(String::new, |label| format!("  · {label}"))
}

/// Cabecera de una línea del chrome `Flat`: marca, directorio, modelo y estado.
fn draw_flat_header(frame: &mut Frame, area: Rect, app: &App) {
    let status_style = if app.status.starts_with("error:") {
        app.theme.styles.error
    } else {
        app.theme.styles.muted
    };
    let mut text = vec![
        Span::styled(
            " CODELOOM ",
            app.theme.styles.heading.add_modifier(Modifier::BOLD),
        ),
        Span::styled("╱╱╱╱", app.theme.styles.muted),
        Span::styled(format!("  {}", app.cwd), app.theme.styles.muted),
        Span::styled(format!("  • {}", app.model), app.theme.styles.muted),
        Span::styled(
            format!("  • {}{}", app.status, background_suffix(app)),
            status_style,
        ),
    ];
    if app.queued > 0 {
        text.push(Span::styled(
            format!("  • {} queued", app.queued),
            app.theme.styles.muted,
        ));
    }
    if app.queue_paused {
        text.push(Span::styled("  • queue paused", app.theme.styles.warning));
    }
    let (running, finished, subagents, subagents_finished) = job_counts(app);
    if running > 0 || finished > 0 || subagents > 0 || subagents_finished > 0 {
        text.push(Span::styled(
            format!("  • {} · {running} jobs", app.running_summary()),
            app.theme.styles.muted,
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(text)), area);
}

/// Atajos del pie del chrome `Flat`, en el orden en que se muestran.
const FLAT_HINTS: [(&str, &str); 8] = [
    ("enter", "send"),
    ("ctrl+j", "newline"),
    ("ctrl+k", "queue"),
    ("ctrl+o", "fold"),
    ("ctrl+t", "tools"),
    ("ctrl+b", "sidebar"),
    ("f2", "settings"),
    ("esc", "cancel"),
];

/// Ancho mínimo del pie para reservar a su derecha el medidor de contexto.
const METER_MIN_FOOTER_WIDTH: u16 = 80;

/// Reserva a la derecha del pie el medidor de contexto, si hay medidor que pintar y el pie tiene
/// al menos `METER_MIN_FOOTER_WIDTH` columnas. Devuelve el área que queda para el texto del pie;
/// sin medidor, o en un pie más estrecho, devuelve el área entera.
fn split_footer_meter(frame: &mut Frame, area: Rect, app: &App) -> Rect {
    if area.width < METER_MIN_FOOTER_WIDTH {
        return area;
    }
    let meter = app.context_meter();
    let width = context_meter::meter_width(&meter, MeterStyle::Segments);
    if width == 0 {
        return area;
    }
    let [left, right] = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(width.min(area.width)),
    ])
    .areas(area);
    context_meter::render(frame, right, &meter, MeterStyle::Segments, &app.theme);
    left
}

/// Pie del chrome `Flat`: el feedback de slash si lo hay; si no, atajos cortos.
fn draw_flat_footer(frame: &mut Frame, area: Rect, app: &App) {
    let area = split_footer_meter(frame, area, app);
    let line = if let Some(feedback) = app.slash_feedback.as_deref() {
        Line::from(Span::styled(feedback, app.theme.styles.muted))
    } else {
        let mut spans = Vec::with_capacity(FLAT_HINTS.len() * 3);
        for (index, (key, label)) in FLAT_HINTS.iter().enumerate() {
            if index > 0 {
                spans.push(Span::styled("  ", app.theme.styles.muted));
            }
            spans.push(Span::styled(*key, app.theme.styles.heading));
            spans.push(Span::styled(format!(" {label}"), app.theme.styles.muted));
        }
        Line::from(spans)
    };
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_transcript(frame: &mut Frame, area: Rect, app: &mut App, panes: &mut Vec<(Focus, Rect)>) {
    panes.push((Focus::Transcript, area));
    app.transcript_area = None;
    // El chrome `Flat` no usa caja: el chat ocupa el área con un margen de una columna.
    let flat = app.theme.look.chrome == ChromeStyle::Flat;
    let inner = if flat {
        Rect::new(
            area.x.saturating_add(1),
            area.y,
            area.width.saturating_sub(2),
            area.height,
        )
    } else {
        batten::widgets::panel::render(
            frame,
            area,
            &batten::widgets::Panel {
                title: "Conversation",
                info: &app.status,
                tone: if app.status == "working" {
                    batten::state::Tone::Info
                } else {
                    batten::state::Tone::Neutral
                },
            },
            &app.theme,
        )
    };
    if inner.is_empty() {
        return;
    }
    // Con el look `Pinned` la última herramienta ocupa una tarjeta fija; con
    // `Inline` aparece dentro del transcript y no reserva filas aquí.
    let pinned = app.theme.look.tool == batten::widgets::ToolStyle::Pinned;
    let thinking_rows = u16::from(app.thinking);
    let max_tool_rows = inner.height.saturating_sub(4 + thinking_rows);
    let tool_rows = app.tools.back().filter(|_| pinned).map_or(0, |tool| {
        let args = usize::from(tool.summary.as_deref().is_some_and(|s| !s.is_empty()));
        let output_lines = tool
            .output
            .as_deref()
            .map_or(0, |output| output.lines().count());
        let visible_output = if tool.expanded {
            output_lines
        } else {
            output_lines.min(2) + usize::from(output_lines > 2)
        };
        let waiting = usize::from(tool.output.is_none());
        (1 + args + visible_output + waiting).min(usize::from(max_tool_rows)) as u16
    });
    let reserved_rows = 2u16.saturating_add(thinking_rows).saturating_add(tool_rows);
    let max_composer_height = inner.height.saturating_sub(reserved_rows);
    let composer_height = batten::widgets::composer::height_for_look(
        &app.theme,
        &app.composer,
        inner.width,
        max_composer_height,
    );
    // El chrome `Flat` no muestra la cabecera de la conversación.
    let header_rows = if flat { 0 } else { 1 };
    let rows = Layout::vertical([
        Constraint::Length(header_rows),
        Constraint::Min(1),
        Constraint::Length(thinking_rows),
        Constraint::Length(tool_rows),
        Constraint::Length(composer_height),
    ])
    .split(inner);
    app.composer_area = Some(rows[4]);
    panes.push((Focus::Composer, rows[4]));
    if !flat {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("CodeLoom", app.theme.styles.heading),
                Span::styled("  ·  project conversation", app.theme.styles.muted),
                Span::styled(
                    format!("  ·  {}", app.theme_preset.name()),
                    app.theme.styles.muted,
                ),
            ])),
            rows[0],
        );
    }
    app.transcript_area = Some(rows[1]);
    batten::widgets::transcript::render(
        frame,
        rows[1],
        &app.transcript,
        &mut app.viewport,
        &mut app.layout,
        app.selection.as_ref(),
        &app.theme,
    );
    // Una selección de texto deja de valer si el diseño cambió de ancho o si ya no
    // cabe en el transcript (p. ej. tras una sesión más corta).
    let stale = app.text_sel.is_some_and(|sel| {
        app.text_sel_width != app.layout.width() || sel.ordered().0.row >= app.layout.total_rows()
    });
    if stale {
        app.text_sel = None;
    }
    if let Some(sel) = app.text_sel.as_ref() {
        batten::widgets::transcript::paint_text_selection(
            frame.buffer_mut(),
            rows[1],
            &app.layout,
            &app.viewport,
            sel,
            app.theme.styles.selection,
        );
    }
    if app.thinking {
        let line = app.reasoning.lines().last().unwrap_or("Thinking…");
        batten::widgets::thinking::render(frame, rows[2], line, &app.animation, &app.theme);
    }
    if let Some(tool) = app.tools.back().filter(|_| pinned) {
        batten::widgets::tool_item::render_crush(frame, rows[3], tool, &app.theme);
    }
    batten::widgets::composer::render_for_look(
        frame,
        rows[4],
        &app.composer,
        app.focus == crate::app::Focus::Composer
            && app.permission.is_none()
            && app.question.is_none()
            && !app.session_picker_open
            && !app.session_loading
            && !app.queue_open
            && !app.settings_open
            && !app.help_open
            && !app.agents_open
            && !app.tools_open,
        &app.model,
        &app.theme,
    );
    if app.slash_query().is_some() {
        let matches = app.slash_matches();
        let commands = matches
            .iter()
            .map(
                |(_, usage, description)| batten::widgets::command_suggestions::MarttyCommand {
                    usage,
                    description,
                    section: None,
                    skill: false,
                },
            )
            .collect::<Vec<_>>();
        let chat = Rect::new(
            inner.x,
            inner.y,
            inner.width,
            rows[4].y.saturating_sub(inner.y),
        );
        batten::widgets::command_suggestions::render_martty(
            frame,
            rows[4],
            chat,
            &commands,
            app.slash_selected,
            &app.theme,
        );
    }
}

fn draw_workspace_metrics(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    panes: &mut Vec<(Focus, Rect)>,
) {
    let rows = Layout::vertical([
        Constraint::Length(6),
        Constraint::Length(5),
        Constraint::Min(3),
    ])
    .split(area);
    // «Token usage» (rows[1]) pertenece a la misma ventana Agent.
    panes.push((Focus::Agent, rows[0]));
    panes.push((Focus::Agent, rows[1]));
    panes.push((Focus::Activity, rows[2]));
    let (agent_info, agent_tone) = if app.status.starts_with("error:") {
        ("error", batten::state::Tone::Error)
    } else if app.status == "working" {
        ("active", batten::state::Tone::Info)
    } else {
        ("idle", batten::state::Tone::Neutral)
    };
    let agent = batten::widgets::panel::render(
        frame,
        rows[0],
        &batten::widgets::Panel {
            title: "Agent",
            info: agent_info,
            tone: agent_tone,
        },
        &app.theme,
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(app.model.as_str()),
            Line::from(format!(
                "{} · {:.1}s",
                app.phase,
                app.phase_duration().as_secs_f64()
            )),
            Line::from(app.phase_summary()),
            Line::from(if app.max_steps > 0 {
                format!("step {}/{}", app.current_step, app.max_steps)
            } else {
                app.status.clone()
            }),
        ])
        .wrap(Wrap { trim: true }),
        agent,
    );
    let context = batten::widgets::panel::render(
        frame,
        rows[1],
        &batten::widgets::Panel {
            title: "Token usage",
            info: "turn/session",
            tone: batten::state::Tone::Neutral,
        },
        &app.theme,
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(format!(
                "Turn ↑{} ↓{}",
                app.turn_input_tokens, app.turn_output_tokens
            )),
            Line::from(format!(
                "Session ↑{} ↓{}",
                app.input_tokens, app.output_tokens
            )),
        ]),
        context,
    );
    let activity = batten::widgets::panel::render(
        frame,
        rows[2],
        &batten::widgets::Panel {
            title: "Activity",
            info: "tools",
            tone: batten::state::Tone::Info,
        },
        &app.theme,
    );
    let entries = app
        .tools
        .iter()
        .map(|tool| batten::widgets::queue::QueueEntry {
            label: tool.name.as_str(),
            status: match tool.status() {
                batten::state::ToolStatus::Pending
                | batten::state::ToolStatus::AwaitingPermission => {
                    batten::widgets::queue::QueueStatus::Pending
                }
                batten::state::ToolStatus::Running => batten::widgets::queue::QueueStatus::Running,
                batten::state::ToolStatus::Success => batten::widgets::queue::QueueStatus::Done,
                batten::state::ToolStatus::Error | batten::state::ToolStatus::Canceled => {
                    batten::widgets::queue::QueueStatus::Failed
                }
            },
        })
        .collect::<Vec<_>>();
    batten::widgets::queue::render_pawan(frame, activity, &entries, app.animation.frame);
}

fn draw_workspace_sidebar(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    panes: &mut Vec<(Focus, Rect)>,
) {
    panes.push((Focus::Sidebar, area));
    let changes_height = if app.changes.is_empty() {
        0
    } else {
        (app.changes.len() as u16 + 2).min(area.height.saturating_sub(1))
    };
    let [sidebar, changes] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(changes_height)]).areas(area);
    batten::widgets::sidebar::render(frame, sidebar, &app.sidebar(), &app.theme);
    if changes_height > 0 {
        batten::widgets::file_changes::render(
            frame,
            changes,
            "ficheros modificados",
            &app.changes,
            &app.theme,
        );
    }
}

fn draw_resize_hint(frame: &mut Frame, area: Rect, title: &str, app: &App) {
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(format!("{title}: enlarge terminal · Esc close"))
            .style(app.theme.styles.warning)
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn draw_session_picker(frame: &mut Frame, area: Rect, app: &App) {
    let width = area.width.min(76).saturating_sub(2);
    let sessions = app.matching_sessions().collect::<Vec<_>>();
    let search_rows = u16::from(app.session_searching);
    let height = (sessions
        .len()
        .saturating_add(usize::from(search_rows))
        .saturating_add(6)
        .min(usize::from(area.height.saturating_sub(2)))) as u16;
    if width < 24 || height < 5 {
        draw_resize_hint(frame, area, "Sessions", app);
        return;
    }
    let popup = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(" Open a session ")
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(app.theme.styles.border);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let [search, list, footer] = Layout::vertical([
        Constraint::Length(search_rows),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(inner);
    if app.session_searching {
        frame.render_widget(
            Paragraph::new(Line::styled(
                format!(
                    "Filter: {}_  ({} matches · Ctrl+F search · Esc clear)",
                    app.session_query,
                    sessions.len()
                ),
                app.theme.styles.muted,
            )),
            search,
        );
    }
    let mut lines = Vec::new();
    lines.extend(sessions.iter().enumerate().map(|(index, session)| {
        let selected = index == app.session_selected;
        Line::from(vec![
            Span::styled(
                if selected { "› " } else { "  " },
                app.theme.styles.accent_from,
            ),
            Span::styled(
                if app.session_id == Some(session.id) {
                    format!("{}  · active", session.preview)
                } else {
                    session.preview.clone()
                },
                if selected {
                    app.theme.styles.heading.add_modifier(Modifier::BOLD)
                } else {
                    app.theme.styles.default
                },
            ),
            Span::styled(
                format!("  · {} msgs", session.message_count),
                app.theme.styles.muted,
            ),
        ])
    }));
    let new_selected = app.session_selected == sessions.len();
    lines.push(Line::from(vec![
        Span::styled(
            if new_selected { "› " } else { "  " },
            app.theme.styles.accent_from,
        ),
        Span::styled(
            "＋ New session",
            if new_selected {
                app.theme.styles.heading.add_modifier(Modifier::BOLD)
            } else {
                app.theme.styles.default
            },
        ),
    ]));
    let scroll = app
        .session_selected
        .saturating_sub(usize::from(list.height.saturating_sub(1)))
        .min(usize::from(u16::MAX)) as u16;
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .scroll((scroll, 0)),
        list,
    );
    frame.render_widget(
        Paragraph::new(Line::styled(
            "↑/↓ or j/k select · Home/End jump · Enter open · N new · Ctrl+F filter · Esc quit",
            app.theme.styles.muted,
        )),
        footer,
    );
}

fn draw_provider_settings(frame: &mut Frame, area: Rect, app: &App) {
    let width = area.width.min(72).saturating_sub(2);
    let height = 12.min(area.height.saturating_sub(2));
    if width < 40 || height < 8 {
        draw_resize_hint(frame, area, "Provider settings", app);
        return;
    }
    let popup = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(" Provider settings ")
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(app.theme.styles.border);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let Some(settings) = &app.provider_settings else {
        return;
    };
    let kind = match settings.kind {
        codeloom_core::config::ProviderKind::OpenAiCompat => "OpenAI compatible",
        codeloom_core::config::ProviderKind::Anthropic => "Anthropic",
    };
    let value = |index: usize, current: &str| {
        if app.settings_editing && app.settings_field == index {
            app.settings_editor.text().to_string()
        } else {
            current.to_string()
        }
    };
    let changes = app.settings_change_count();
    let save_label = match changes {
        0 => "Save and apply".to_string(),
        1 => "Save and apply (1 change)".to_string(),
        n => format!("Save and apply ({n} changes)"),
    };
    let mut rows: Vec<(&str, String)> = vec![
        ("Provider", kind.to_string()),
        ("Model", value(1, &settings.model)),
        ("Base URL", value(2, &settings.base_url)),
        ("API key env", value(3, &settings.api_key_env)),
    ];
    // «Effort» solo aparece si el modelo admite niveles; si no, «Save» ocupa su posición.
    if app.settings_effort_visible() {
        rows.push((
            "Effort",
            settings
                .effort
                .clone()
                .unwrap_or_else(|| "default".to_string()),
        ));
    }
    rows.push((
        save_label.as_str(),
        if app.settings_saving {
            "Applying…"
        } else {
            "Enter"
        }
        .to_string(),
    ));
    let mut lines = rows
        .iter()
        .enumerate()
        .map(|(index, (label, value))| {
            let selected = app.settings_field == index;
            let mut spans = vec![
                Span::styled(
                    if selected { "› " } else { "  " },
                    app.theme.styles.accent_from,
                ),
                Span::styled(
                    format!("{label}: "),
                    if selected {
                        app.theme.styles.heading.add_modifier(Modifier::BOLD)
                    } else {
                        app.theme.styles.muted
                    },
                ),
                Span::styled(value.as_str(), app.theme.styles.default),
            ];
            if app.settings_field_changed(index) {
                spans.push(Span::styled(" ● unsaved", app.theme.styles.warning));
            }
            Line::from(spans)
        })
        .collect::<Vec<_>>();
    if let Some(feedback) = &app.settings_feedback {
        lines.push(Line::styled(feedback.clone(), app.theme.styles.warning));
    }
    lines.push(Line::styled(
        if app.settings_editing {
            "Ctrl+A clear · Enter accept · Esc cancel"
        } else if app.settings_saving {
            "Applying settings… · Esc close"
        } else if app.settings_picker.is_some() {
            "type to filter · ↑/↓ select · Enter choose · Esc close list"
        } else {
            "↑/↓ select · ←/→ provider/effort · Enter edit/apply · Ctrl+L catalog · Ctrl+S save (idle) · Esc close"
        },
        app.theme.styles.muted,
    ));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
    if app.settings_picker.is_some() {
        draw_settings_picker(frame, popup, app);
    }
    if app.settings_editing {
        let label_width = [
            "Provider: ".len(),
            "Model: ".len(),
            "Base URL: ".len(),
            "API key env: ".len(),
            "Effort: ".len(),
            "Save and apply: ".len(),
        ][app.settings_field];
        let (_, col) = app.settings_editor.cursor_row_col();
        let x = inner.x + 2 + label_width as u16 + col as u16;
        let y = inner.y + app.settings_field as u16;
        if x < inner.right() && y < inner.bottom() {
            frame.set_cursor_position((x, y));
        }
    }
}

/// Filas visibles del selector de `/settings` antes de desplazarse.
const PICKER_VISIBLE_ROWS: usize = 8;

/// Pinta el selector de `/settings` como una lista encima del diálogo, con el filtro arriba.
fn draw_settings_picker(frame: &mut Frame, dialog: Rect, app: &App) {
    let Some(picker) = app.settings_picker else {
        return;
    };
    let items = app.picker_items();
    let rows = items.len().clamp(1, PICKER_VISIBLE_ROWS) as u16;
    // Bordes (2) y línea de filtro (1); nunca cubre la primera fila del diálogo.
    let height = (rows + 3).min(dialog.height.saturating_sub(1));
    if height < 3 {
        return;
    }
    let area = Rect::new(
        dialog.x + 1,
        dialog.y + 1,
        dialog.width.saturating_sub(2),
        height,
    );
    frame.render_widget(Clear, area);
    let title = match picker {
        SettingsPicker::Provider => " Provider ",
        SettingsPicker::Model => " Model ",
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(app.theme.styles.border);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = vec![Line::from(vec![
        Span::styled("Filter: ", app.theme.styles.muted),
        Span::styled(app.picker_query.clone(), app.theme.styles.default),
    ])];
    if items.is_empty() {
        lines.push(Line::styled("No matches", app.theme.styles.muted));
    }
    // El desplazamiento mantiene visible la fila resaltada.
    let selected = app.picker_selected.min(items.len().saturating_sub(1));
    let offset = (selected + 1).saturating_sub(PICKER_VISIBLE_ROWS);
    for (index, item) in items
        .iter()
        .enumerate()
        .skip(offset)
        .take(PICKER_VISIBLE_ROWS)
    {
        let is_selected = index == selected;
        lines.push(Line::from(vec![
            Span::styled(
                if is_selected { "› " } else { "  " },
                app.theme.styles.accent_from,
            ),
            Span::styled(
                item.label.clone(),
                if is_selected {
                    app.theme.styles.heading.add_modifier(Modifier::BOLD)
                } else {
                    app.theme.styles.default
                },
            ),
            Span::styled(format!("  {}", item.detail), app.theme.styles.muted),
        ]));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_prompt_queue(frame: &mut Frame, area: Rect, app: &App) {
    let width = area.width.min(78).saturating_sub(2);
    let height = (app
        .pending_prompts
        .len()
        .saturating_add(10)
        .max(11)
        .min(usize::from(area.height.saturating_sub(2)))) as u16;
    if width < 62 || height < 11 {
        draw_resize_hint(frame, area, "Prompt queue", app);
        return;
    }
    let popup = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(format!(
            " Prompt queue ({}) · {} ",
            app.pending_prompts.len(),
            if app.queue_paused {
                "paused"
            } else {
                "resumes on close"
            },
        ))
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(app.theme.styles.border);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let [list, preview, footer] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(5),
        Constraint::Length(3),
    ])
    .areas(inner);
    let mut lines = app
        .pending_prompts
        .iter()
        .enumerate()
        .map(|(index, prompt)| {
            let prefix = if index == app.queue_selected {
                "› "
            } else {
                "  "
            };
            let text = prompt.lines().next().unwrap_or("");
            let clipped = text
                .chars()
                .take(list.width.saturating_sub(3) as usize)
                .collect::<String>();
            Line::styled(
                format!("{prefix}{clipped}"),
                if index == app.queue_selected {
                    app.theme.styles.heading.add_modifier(Modifier::BOLD)
                } else {
                    app.theme.styles.default
                },
            )
        })
        .collect::<Vec<_>>();
    if lines.is_empty() {
        lines.push(Line::styled("Queue is empty", app.theme.styles.muted));
    }
    let scroll = app
        .queue_selected
        .saturating_sub(usize::from(list.height.saturating_sub(1))) as u16;
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .scroll((scroll, 0)),
        list,
    );
    let selected = app.pending_prompts.get(app.queue_selected);
    let text = selected.map_or("Select a queued prompt", String::as_str);
    if app.queue_editing {
        batten::widgets::composer::render_focused(
            frame,
            preview,
            &app.queue_editor,
            true,
            &app.theme,
        );
    } else {
        frame.render_widget(
            Paragraph::new(text)
                .block(Block::default().title(" Preview ").borders(Borders::TOP))
                .wrap(Wrap { trim: true }),
            preview,
        );
    }
    frame.render_widget(
        Paragraph::new(if app.queue_editing {
            "Enter accept (nonempty) · Ctrl+J newline\nCtrl+A clear · Esc discard edit"
        } else {
            "↑/↓ select · Ctrl+↑/↓ move · Del remove · Enter edit\nSpace pause/resume · Ctrl+X cancel active and pause\nEsc close · Pending prompts wait while this dialog is open"
        })
            .style(app.theme.styles.muted)
            .wrap(Wrap { trim: true }),
        footer,
    );
}

fn draw_tool_inspector(frame: &mut Frame, area: Rect, app: &mut App) {
    let width = area.width.min(100).saturating_sub(2);
    let height = area.height.saturating_sub(2);
    if width < 54 || height < 13 {
        draw_resize_hint(frame, area, "Tool inspector", app);
        return;
    }
    let popup = Rect::new(area.x + (area.width - width) / 2, area.y + 1, width, height);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(" Tool inspector · last 8 calls ")
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(app.theme.styles.border);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let [list, detail, footer] = Layout::vertical([
        Constraint::Length(app.tools.len().clamp(1, 4) as u16),
        Constraint::Min(3),
        Constraint::Length(3),
    ])
    .areas(inner);
    let rows = if app.tools.is_empty() {
        vec![Line::styled("No tool calls yet", app.theme.styles.muted)]
    } else {
        app.tools
            .iter()
            .enumerate()
            .map(|(index, tool)| {
                Line::styled(
                    format!(
                        "{} {} · {:?}",
                        if index == app.tool_selected {
                            "›"
                        } else {
                            " "
                        },
                        tool.name,
                        tool.status()
                    ),
                    if index == app.tool_selected {
                        app.theme.styles.heading
                    } else {
                        app.theme.styles.muted
                    },
                )
            })
            .collect()
    };
    frame.render_widget(
        Paragraph::new(rows).scroll((
            app.tool_selected
                .saturating_sub(usize::from(list.height.saturating_sub(1))) as u16,
            0,
        )),
        list,
    );
    let mut lines = Vec::new();
    if let Some(tool) = app.tools.get(app.tool_selected) {
        lines.push(Line::styled("Arguments", app.theme.styles.heading));
        lines.extend(
            tool.summary
                .as_deref()
                .unwrap_or("No arguments received")
                .split('\n')
                .map(Line::from),
        );
        lines.push(Line::styled("Output", app.theme.styles.heading));
        lines.extend(
            tool.output
                .as_deref()
                .unwrap_or("No output received")
                .split('\n')
                .map(Line::from),
        );
    }
    let max_y = lines
        .len()
        .saturating_sub(usize::from(detail.height))
        .min(usize::from(u16::MAX)) as u16;
    let max_x = lines
        .iter()
        .map(Line::width)
        .max()
        .unwrap_or(0)
        .saturating_sub(usize::from(detail.width))
        .min(usize::from(u16::MAX)) as u16;
    app.tool_scroll.0 = app.tool_scroll.0.min(max_y);
    app.tool_scroll.1 = app.tool_scroll.1.min(max_x);
    frame.render_widget(Paragraph::new(lines).scroll(app.tool_scroll), detail);
    frame.render_widget(
        Paragraph::new("↑/↓ select · PgUp/PgDn scroll · ←/→ pan\nHome/End start/end · Y copy output\nEsc or Ctrl+T close")
            .style(app.theme.styles.muted),
        footer,
    );
}

fn draw_help_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let width = area.width.min(78).saturating_sub(2);
    // Diecisiete líneas de contenido más el marco; por eso el recuadro crece a 21 filas.
    let height = area.height.min(21).saturating_sub(2);
    if width < 54 || height < 19 {
        draw_resize_hint(frame, area, "Keyboard shortcuts", app);
        return;
    }
    let popup = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(" Keyboard shortcuts ")
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(app.theme.styles.border);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from("Enter       Send prompt"),
            Line::from("Ctrl+J      Insert newline"),
            Line::from("Tab/Shift+Tab Switch composer/transcript"),
            Line::from("↑/↓ + Enter Open subagent (rail focused)"),
            Line::from("Alt+←/parent Back to the parent session"),
            Line::from("Mouse wheel Scroll transcript"),
            Line::from("Ctrl+B      Toggle sidebar at medium width"),
            Line::from("Ctrl+K      Open prompt queue"),
            Line::from("Ctrl+O      Expand latest tool output"),
            Line::from("Ctrl+T      Inspect recent tool calls"),
            Line::from("/           Commands at start of composer"),
            Line::from("/theme      Cycle terminal color theme"),
            Line::from("F3          Switch or create session (when idle)"),
            Line::from("F2          Provider settings"),
            Line::from("Esc         Cancel turn; press twice to quit"),
            Line::from("Ctrl+C/Q    Quit"),
            Line::styled("Press F1 or Esc to close", app.theme.styles.muted),
        ])
        .wrap(Wrap { trim: true }),
        inner,
    );
}

/// Texto del estado de aprobación, tal como se rotula en el popup de agentes.
fn approval_label(approval: codeloom_core::registry::Approval) -> &'static str {
    use codeloom_core::registry::Approval;
    match approval {
        Approval::NotRequired => "usable",
        Approval::Approved => "approved",
        Approval::Pending => "awaiting approval",
    }
}

/// Popup de `/agents`: la lista del registro con el agente seleccionado y, debajo, su detalle
/// (origen y prompt), que es lo que se revisa antes de aprobar un agente de proyecto.
fn draw_agents_overlay(frame: &mut Frame, area: Rect, app: &App) {
    let width = area.width.min(100).saturating_sub(2);
    let height = area.height.min(30).saturating_sub(2);
    if width < 40 || height < 10 {
        draw_resize_hint(frame, area, "Agents", app);
        return;
    }
    let popup = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(" Agents ")
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(app.theme.styles.border);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    // Reparto vertical: lista (la mitad), detalle (el resto) y dos líneas de pie.
    let util = inner.height.saturating_sub(2);
    let alto_lista = (util / 2).max(2);
    let por_pagina = (alto_lista / 2).max(1) as usize;
    let desde = app
        .agents_selected
        .saturating_sub(por_pagina.saturating_sub(1));
    let mut lineas: Vec<Line> = Vec::new();
    for (indice, fila) in app
        .agent_rows
        .iter()
        .enumerate()
        .skip(desde)
        .take(por_pagina)
    {
        let marca = if indice == app.agents_selected {
            "›"
        } else {
            " "
        };
        let cabecera = format!(
            "{marca} {} [{}] {} — {}",
            fila.name,
            approval_label(fila.approval),
            fila.origin,
            fila.execution
        );
        let estilo = if indice == app.agents_selected {
            app.theme.styles.border
        } else {
            Style::default()
        };
        lineas.push(Line::styled(cabecera, estilo));
        lineas.push(Line::styled(
            format!("    {}", fila.description),
            app.theme.styles.muted,
        ));
    }
    if app.agent_rows.is_empty() {
        lineas.push(Line::from("No agents loaded (no registry available)"));
    }
    while lineas.len() < alto_lista as usize {
        lineas.push(Line::from(""));
    }
    lineas.truncate(alto_lista as usize);
    if let Some(fila) = app.agent_rows.get(app.agents_selected) {
        lineas.push(Line::styled(
            format!("── Prompt of `{}` ──", fila.name),
            app.theme.styles.muted,
        ));
        let restante = util.saturating_sub(alto_lista).saturating_sub(1) as usize;
        if fila.body.trim().is_empty() {
            lineas.push(Line::styled(
                "(empty: inherits the parent's behaviour)",
                app.theme.styles.muted,
            ));
        } else {
            lineas.extend(fila.body.lines().take(restante).map(Line::from));
        }
    }
    let pie = app.agents_status.clone().unwrap_or_default();
    let mut todo = lineas;
    // El pie va siempre en las dos últimas filas del interior.
    todo.truncate(util as usize);
    while todo.len() < util as usize {
        todo.push(Line::from(""));
    }
    todo.push(Line::styled(pie, app.theme.styles.muted));
    todo.push(Line::styled(
        "↑/↓ select · a approve · r revoke · Esc close",
        app.theme.styles.muted,
    ));
    frame.render_widget(Paragraph::new(todo), inner);
}

/// Glifos de borde que el indicador de foco recolorea (nunca el texto del título).
const BORDER_GLYPHS: [&str; 6] = ["─", "│", "╭", "╮", "╰", "╯"];

/// Recolorea con el acento del tema el borde de la celda (x, y), si lo es.
fn recolor_border_cell(buf: &mut Buffer, x: u16, y: u16, accent: Style) {
    if let Some(cell) = buf.cell_mut((x, y)) {
        if BORDER_GLYPHS.contains(&cell.symbol()) {
            cell.set_style(accent);
        }
    }
}

/// Recolorea el borde izquierdo de una ventana (sidebar y rail solo tienen ese).
fn recolor_left_edge(buf: &mut Buffer, area: Rect, accent: Style) {
    for y in area.top()..area.bottom() {
        if let Some(cell) = buf.cell_mut((area.left(), y)) {
            if cell.symbol() == "│" {
                cell.set_style(accent);
            }
        }
    }
}

/// Recolorea el marco completo de una ventana con caja.
fn recolor_frame(buf: &mut Buffer, area: Rect, accent: Style) {
    if area.width < 2 || area.height < 2 {
        return;
    }
    let (left, right) = (area.left(), area.right() - 1);
    let (top, bottom) = (area.top(), area.bottom() - 1);
    for x in left..=right {
        recolor_border_cell(buf, x, top, accent);
        recolor_border_cell(buf, x, bottom, accent);
    }
    for y in top..=bottom {
        recolor_border_cell(buf, left, y, accent);
        recolor_border_cell(buf, right, y, accent);
    }
}

/// Marca con un filete de acento la columna de margen de una ventana sin caja.
fn mark_margin(buf: &mut Buffer, area: Rect, accent: Style) {
    for y in area.top()..area.bottom() {
        if let Some(cell) = buf.cell_mut((area.left(), y)) {
            cell.set_symbol("▎");
            cell.set_style(accent);
        }
    }
}

/// Indicador de foco: resalta la ventana con `app.focus`. El composer no se
/// marca, porque ya muestra su propio cursor. Con el chrome `Flat` (Crush) no
/// hay caja: el margen del transcript recibe un filete y el rail recolorea su
/// borde izquierdo.
fn mark_focused_panes(frame: &mut Frame, panes: &[(Focus, Rect)], app: &App, flat: bool) {
    let accent = app.theme.styles.accent;
    let buf = frame.buffer_mut();
    for (pane, area) in panes {
        if *pane != app.focus || *pane == Focus::Composer || area.is_empty() {
            continue;
        }
        match pane {
            Focus::Sidebar | Focus::Rail => recolor_left_edge(buf, *area, accent),
            // Con `Flat` el transcript no pinta filete: sus líneas por bloque ya marcan el foco.
            Focus::Transcript if flat => {}
            _ if flat => mark_margin(buf, *area, accent),
            _ => recolor_frame(buf, *area, accent),
        }
    }
}

/// Línea fina del margen para un bloque inactivo. Solo los mensajes de usuario la
/// llevan; el resto de roles queda sin línea hasta que el bloque se activa.
fn rail_style(kind: batten::state::LineKind, app: &App) -> Option<Style> {
    use batten::state::LineKind;
    match kind {
        LineKind::User => Some(app.theme.styles.accent),
        _ => None,
    }
}

/// Color del marcador grueso del bloque activo, según el rol de su primera línea:
/// el acento para el usuario y un verde suave (éxito) para el resto.
fn active_marker_style(kind: batten::state::LineKind, app: &App) -> Style {
    use batten::state::LineKind;
    if matches!(kind, LineKind::User) {
        app.theme.styles.accent
    } else {
        app.theme.styles.success
    }
}

/// Pinta una línea vertical fina en la columna de margen por cada bloque de
/// usuario visible del transcript. Solo con el chrome `Flat`. Recorre únicamente
/// las entradas que caen en la vista y parchea sus celdas: no re-renderiza nada.
fn mark_block_rails(frame: &mut Frame, app: &App, flat: bool) {
    if !flat {
        return;
    }
    let Some(area) = app
        .transcript_area
        .filter(|area| !area.is_empty() && area.x > 0)
    else {
        return;
    };
    let margin = area.x - 1;
    let top = app.viewport.top();
    let bottom = top + usize::from(area.height);
    let Some((line, _)) = app.layout.line_at_row(top) else {
        return;
    };
    let Some((first, _)) = app.transcript.locate(line) else {
        return;
    };
    let buf = frame.buffer_mut();
    for entry in first..app.transcript.entry_count() {
        let first_line = app.transcript.entry_start(entry);
        let start = app.layout.row_of(first_line);
        if start >= bottom {
            break;
        }
        let lines = app.transcript.entry_lines(entry);
        let end = app.layout.row_of(first_line + lines.len());
        let Some(style) = lines.first().and_then(|line| rail_style(line.kind, app)) else {
            continue;
        };
        for row in start.max(top)..end.min(bottom) {
            let Ok(dy) = u16::try_from(row - top) else {
                break;
            };
            if let Some(cell) = buf.cell_mut((margin, area.y + dy)) {
                cell.set_symbol("│").set_style(style);
            }
        }
    }
}

/// Marca las filas visibles del bloque activo del transcript. Solo con el
/// transcript enfocado. Con el chrome `Flat` pinta un marcador grueso `▌` en la
/// columna de margen, sobre la línea por rol: acento si es de usuario y éxito
/// suave en el resto. Sin margen recolorea la primera celda de cada fila con el
/// acento. Solo parchea el búfer ya pintado: no re-renderiza nada.
fn mark_active_block(frame: &mut Frame, app: &App, flat: bool) {
    if app.focus != Focus::Transcript {
        return;
    }
    let Some(area) = app.transcript_area.filter(|area| !area.is_empty()) else {
        return;
    };
    let Some(block) = app.block_index() else {
        return;
    };
    let first = app.transcript.entry_start(block);
    let start = app.layout.row_of(first);
    let end = app
        .layout
        .row_of(first + app.transcript.entry_lines(block).len());
    let top = app.viewport.top();
    let from = start.max(top);
    let to = end.min(top + usize::from(area.height));
    let accent = app.theme.styles.accent;
    let marker = app
        .transcript
        .entry_lines(block)
        .first()
        .map_or(accent, |line| active_marker_style(line.kind, app));
    let buf = frame.buffer_mut();
    for row in from..to {
        let Ok(dy) = u16::try_from(row - top) else {
            break;
        };
        let y = area.y + dy;
        if flat {
            // El margen es la columna que deja `draw_transcript` a la izquierda.
            if let Some(cell) = buf.cell_mut((area.x.saturating_sub(1), y)) {
                cell.set_symbol("▌").set_style(marker);
            }
        } else if let (Some(cell), Some(fg)) = (buf.cell_mut((area.x, y)), accent.fg) {
            cell.set_fg(fg);
        }
    }
}

pub fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    app.matrix_rain.render(frame, area);
    // El mapa de ventanas reutiliza el mismo `Vec` en cada fotograma.
    let mut panes = std::mem::take(&mut app.pane_rects);
    panes.clear();
    app.composer_area = None;
    let flat = app.theme.look.chrome == ChromeStyle::Flat;
    let rows = Layout::vertical([
        Constraint::Length(if flat {
            1
        } else {
            workspace_header_height(app)
        }),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);
    if flat {
        draw_flat_header(frame, rows[0], app);
    } else {
        draw_workspace_header(frame, rows[0], app);
    }
    if app.theme.look.layout == LayoutStyle::ContextRail {
        // Sin sidebar izquierdo ni métricas: el chat y, si hay ancho, el rail derecho.
        if rows[1].width >= 80 && !app.rail_hidden {
            let [chat, context] =
                Layout::horizontal([Constraint::Min(1), Constraint::Length(32)]).areas(rows[1]);
            draw_transcript(frame, chat, app, &mut panes);
            rail::draw_context_rail(frame, context, app);
            panes.push((Focus::Rail, context));
        } else {
            draw_transcript(frame, rows[1], app, &mut panes);
        }
    } else if rows[1].width >= 105 {
        let [left, center, right] = Layout::horizontal([
            Constraint::Length(23),
            Constraint::Min(42),
            Constraint::Length(30),
        ])
        .areas(rows[1]);
        draw_workspace_sidebar(frame, left, app, &mut panes);
        draw_transcript(frame, center, app, &mut panes);
        draw_workspace_metrics(frame, right, app, &mut panes);
    } else if rows[1].width >= 68 {
        if app.sidebar_expanded {
            let [left, center] =
                Layout::horizontal([Constraint::Length(23), Constraint::Min(1)]).areas(rows[1]);
            draw_workspace_sidebar(frame, left, app, &mut panes);
            draw_transcript(frame, center, app, &mut panes);
        } else {
            let [center, right] =
                Layout::horizontal([Constraint::Min(1), Constraint::Length(28)]).areas(rows[1]);
            draw_transcript(frame, center, app, &mut panes);
            draw_workspace_metrics(frame, right, app, &mut panes);
        }
    } else {
        draw_transcript(frame, rows[1], app, &mut panes);
    }
    mark_focused_panes(frame, &panes, app, flat);
    // Una línea por bloque en el margen; después el bloque activo, que la sobreescribe.
    mark_block_rails(frame, app, flat);
    mark_active_block(frame, app, flat);
    app.pane_rects = panes;
    if flat {
        draw_flat_footer(frame, rows[2], app);
    } else {
        let text_area = split_footer_meter(frame, rows[2], app);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!(" {}{} ", app.status, background_suffix(app)),
                    app.theme.styles.heading,
                ),
                Span::styled(
                    app.slash_feedback.as_deref().unwrap_or(
                        "  Enter send · Ctrl+J newline · Ctrl+K queue · Ctrl+O tool · Ctrl+T inspect · F2 settings · Esc cancel",
                    ),
                    app.theme.styles.muted,
                ),
                Span::styled(
                    format!("  ↑ {} · ↓ {} tokens", app.input_tokens, app.output_tokens),
                    app.theme.styles.muted,
                ),
            ])),
            text_area,
        );
    }

    // Las preguntas se pintan antes que los permisos: un permiso queda por encima.
    if let Some(q) = &app.question {
        batten::widgets::question::render(frame, area, &q.dialog, &app.theme);
    }
    if let Some(p) = &app.permission {
        batten::widgets::permission::render(frame, area, &p.dialog, &app.theme);
    }
    if app.session_picker_open {
        draw_session_picker(frame, area, app);
    } else if app.queue_open {
        draw_prompt_queue(frame, area, app);
    } else if app.session_loading {
        let width = area.width.min(24);
        let popup = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + area.height / 2,
            width,
            u16::from(area.height > 0),
        );
        frame.render_widget(
            Paragraph::new("Loading session…").alignment(Alignment::Center),
            popup,
        );
    }
    if app.settings_open {
        draw_provider_settings(frame, area, app);
    }
    if app.help_open {
        draw_help_overlay(frame, area, app);
    }
    if app.agents_open {
        draw_agents_overlay(frame, area, app);
    }
    if app.tools_open {
        draw_tool_inspector(frame, area, app);
    }
    if app.slash_query().is_some() {
        frame.render_widget(
            Paragraph::new(if let Some(feedback) = app.slash_feedback.as_deref() {
                feedback
            } else if app.slash_matches().is_empty() {
                "Unknown command · Esc to send as text"
            } else {
                "↑/↓ select · Tab complete · Enter run · Esc dismiss"
            })
            .style(app.theme.styles.muted),
            rows[2],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use codeloom_core::task::{TaskState, TaskStatus};
    use ratatui::backend::{Backend, TestBackend};
    use ratatui::Terminal;

    fn buffer_of(app: &mut App, w: u16, h: u16) -> String {
        let backend = TestBackend::new(w, h);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f, app)).unwrap();
        term.backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    /// Última fila de la pantalla (el pie) tras pintar la app entera.
    fn last_row_of(app: &mut App, w: u16, h: u16) -> String {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| render(f, app)).unwrap();
        let buffer = term.backend().buffer();
        let start = usize::from(h - 1) * usize::from(w);
        buffer.content[start..start + usize::from(w)]
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    /// App con el tema indicado, un modelo de ventana conocida y una última petición de 142 000
    /// tokens. `model` elige entre `windowed` (200 000) y `bare` (sin ventana).
    fn footer_app(preset: batten::widgets::ThemePreset, model: &str) -> App {
        let mut app = App::new();
        app.set_theme(preset);
        app.set_catalog(crate::app::windowed_sample_catalog());
        app.set_provider_settings(crate::app::ProviderSettings {
            kind: codeloom_core::config::ProviderKind::OpenAiCompat,
            model: model.into(),
            base_url: "https://acme.example/v1".into(),
            api_key_env: "ACME_KEY".into(),
            effort: None,
        });
        app.apply(codeloom_core::event::LiveEvent::Usage {
            input_tokens: 142_000,
            output_tokens: 1,
        });
        app
    }

    #[test]
    fn footer_shows_the_meter_when_wide_enough() {
        use batten::widgets::ThemePreset;
        // Pie clásico (Batten) y pie plano (Crush): los dos reservan el medidor.
        for preset in [ThemePreset::Batten, ThemePreset::Crush] {
            let mut app = footer_app(preset, "windowed");
            let row = last_row_of(&mut app, 80, 24);
            assert!(row.contains("━━━━━━━━━━"), "medidor en {preset:?}: {row}");
            assert!(row.contains("142k/200k"), "fracción en {preset:?}: {row}");
            assert!(row.contains("71 %"), "porcentaje en {preset:?}: {row}");
        }
    }

    #[test]
    fn footer_hides_the_meter_when_narrow() {
        use batten::widgets::ThemePreset;
        for preset in [ThemePreset::Batten, ThemePreset::Crush] {
            let mut app = footer_app(preset, "windowed");
            let row = last_row_of(&mut app, 79, 24);
            assert!(!row.contains("━"), "sin medidor en {preset:?}: {row}");
            assert!(
                !row.contains("142k/200k"),
                "sin fracción en {preset:?}: {row}"
            );
        }
    }

    #[test]
    fn footer_shows_the_count_when_window_unknown() {
        use batten::widgets::ThemePreset;
        let mut app = footer_app(ThemePreset::Batten, "bare");
        let row = last_row_of(&mut app, 80, 24);
        assert!(row.contains("ctx 142k (sin ventana)"), "{row}");
        assert!(!row.contains("━"), "sin barra sin ventana: {row}");
    }

    #[test]
    fn background_events_drive_the_status_and_the_arrow() {
        use batten::widgets::ThemePreset;
        use codeloom_core::event::LiveEvent;
        let mut app = footer_app(ThemePreset::Batten, "windowed");
        // Sin resumen anticipado no hay texto de estado ni flecha en el pie. La flecha de
        // «ahorro pendiente» se mira junto al porcentaje, porque el pie ya lleva otras «↓».
        let row = last_row_of(&mut app, 80, 24);
        assert!(!row.contains("resumiendo"), "{row}");
        assert!(!row.contains("71 % ↓"), "{row}");

        // Arranca el resumen: la línea de estado cuenta «resumiendo…» mientras corre.
        app.apply(LiveEvent::BackgroundCompactionStarted {
            tokens_before: 120_000,
        });
        let row = last_row_of(&mut app, 80, 24);
        assert!(row.contains("resumiendo…"), "{row}");
        assert!(!row.contains("71 % ↓"), "{row}");

        // El resumen está listo: desaparece el texto de estado y el medidor lleva la flecha.
        app.apply(LiveEvent::BackgroundCompactionReady { summary_bytes: 400 });
        let row = last_row_of(&mut app, 80, 24);
        assert!(row.contains("71 % ↓"), "{row}");
        assert!(!row.contains("resumiendo"), "{row}");
    }

    #[test]
    fn compacted_clears_the_ready_mark() {
        use batten::widgets::ThemePreset;
        use codeloom_core::event::{Activation, LiveEvent};
        let mut app = footer_app(ThemePreset::Batten, "windowed");
        app.apply(LiveEvent::BackgroundCompactionStarted {
            tokens_before: 120_000,
        });
        app.apply(LiveEvent::BackgroundCompactionReady { summary_bytes: 400 });
        assert!(app.context_meter().ready);

        // Al activarse la compactación, la flecha desaparece y el resumen pasa a ser memoria.
        app.apply(LiveEvent::ContextCompacted {
            replaced: 4,
            tokens_before: 160_000,
            summary_bytes: 100,
            activation: Activation::Sync,
        });
        assert!(!app.context_meter().ready);
        let row = last_row_of(&mut app, 80, 24);
        assert!(row.contains("resumen 25"), "{row}");
        assert!(!row.contains("resumen 25 ↓"), "{row}");
        assert!(!row.contains("resumiendo"), "{row}");
    }

    #[test]
    fn renders_items_and_input() {
        let mut app = App::new();
        app.push_user("hello");
        app.push_char('x');
        let content = buffer_of(&mut app, 40, 10);
        assert!(content.contains("hello"));
        assert!(content.contains('x'));
        assert!(content.contains("CodeLoom"));
        assert!(content.contains("CODELOOM / WORKSPACE"));
    }

    #[test]
    fn assistant_markdown_is_rendered() {
        let mut app = App::new();
        app.push_assistant("## Hecho\n\n- **uno**\n- `dos`");
        let content = buffer_of(&mut app, 60, 20);
        assert!(content.contains("Hecho"));
        assert!(!content.contains("##"));
        assert!(content.contains("• uno"));
        assert!(!content.contains("**"));
    }

    #[test]
    fn header_shows_queued_prompts() {
        let mut app = App::new();
        app.queued = 2;
        assert!(buffer_of(&mut app, 60, 10).contains("2 queued"));
    }

    #[test]
    fn header_job_counts_are_scoped_to_the_open_session() {
        let mut app = App::new();
        app.session_id = Some(codeloom_core::session::SessionId(2));
        for (id, owner, status) in [
            ("shell-1", 2, codeloom_core::job::JobStatus::Running),
            ("shell-2", 2, codeloom_core::job::JobStatus::Failed),
            ("shell-3", 1, codeloom_core::job::JobStatus::Running),
            ("subagent-1", 2, codeloom_core::job::JobStatus::Running),
        ] {
            let id = codeloom_core::job::JobId(id.into());
            let kind = if id.0.starts_with("subagent-") {
                "subagent"
            } else {
                "shell"
            };
            app.jobs.insert(
                id.clone(),
                codeloom_core::job::JobSnapshot {
                    id,
                    kind: kind.into(),
                    label: "test".into(),
                    owner: codeloom_core::session::SessionId(owner),
                    status,
                    progress: None,
                    output: String::new(),
                    output_start: 0,
                    output_end: 0,
                    detail: None,
                    result: None,
                },
            );
        }
        for (child, status) in [
            (99, codeloom_core::task::TaskState::Working),
            (100, codeloom_core::task::TaskState::Failed),
        ] {
            app.subagents.insert(
                child,
                codeloom_core::session::SubagentDescriptor {
                    parent: codeloom_core::session::SessionId(2),
                    child: codeloom_core::session::SessionId(child),
                    job_id: format!("subagent-{child}"),
                    label: "test".into(),
                    status: codeloom_core::task::TaskStatus::new(status),
                    artifacts: Vec::new(),
                    notice_delivered: false,
                    agent: None,
                    agent_origin: None,
                },
            );
        }
        assert_eq!(job_counts(&app), (1, 1, 1, 1));
    }

    #[test]
    fn header_names_the_a2a_states_of_its_subagent_counts() {
        let mut app = App::new();
        app.session_id = Some(codeloom_core::session::SessionId(1));
        for (child, state) in [(9, TaskState::Working), (10, TaskState::Completed)] {
            app.subagents.insert(
                child,
                codeloom_core::session::SubagentDescriptor {
                    parent: codeloom_core::session::SessionId(1),
                    child: codeloom_core::session::SessionId(child),
                    job_id: format!("subagent-{child}"),
                    label: "review".into(),
                    status: TaskStatus::new(state),
                    artifacts: Vec::new(),
                    notice_delivered: false,
                    agent: None,
                    agent_origin: None,
                },
            );
        }
        let texto_de = |dibujar: fn(&mut Frame, Rect, &App)| {
            let mut term = Terminal::new(TestBackend::new(260, 3)).unwrap();
            term.draw(|f| dibujar(f, f.area(), &app)).unwrap();
            term.backend()
                .buffer()
                .content
                .iter()
                .map(|celda| celda.symbol())
                .collect::<String>()
        };
        let cabecera = texto_de(draw_workspace_header);
        assert!(
            cabecera.contains("1 subagents running (working 1)"),
            "falta el recuento en curso: {cabecera}"
        );
        // Solo la cabecera de espacio de trabajo cuenta los terminados.
        assert!(
            cabecera.contains(" (subagents: completed 1)"),
            "falta el recuento de terminados: {cabecera}"
        );
        let plana = texto_de(draw_flat_header);
        assert!(
            plana.contains("1 subagents running (working 1)"),
            "falta el recuento en curso: {plana}"
        );
    }

    #[test]
    fn header_shows_state_counts_without_truncation_at_100_columns() {
        let mut app = App::new();
        app.session_id = Some(codeloom_core::session::SessionId(1));
        for (child, state) in [(9, TaskState::Working), (10, TaskState::InputRequired)] {
            app.subagents.insert(
                child,
                codeloom_core::session::SubagentDescriptor {
                    parent: codeloom_core::session::SessionId(1),
                    child: codeloom_core::session::SessionId(child),
                    job_id: format!("subagent-{child}"),
                    label: "review".into(),
                    status: TaskStatus::new(state),
                    artifacts: Vec::new(),
                    notice_delivered: false,
                    agent: None,
                    agent_origin: None,
                },
            );
        }
        let mut term = Terminal::new(TestBackend::new(100, 3)).unwrap();
        term.draw(|f| draw_workspace_header(f, f.area(), &app))
            .unwrap();
        let buffer = term.backend().buffer().clone();
        let fila_resumen: String = buffer.content[100..200]
            .iter()
            .map(|celda| celda.symbol())
            .collect();
        assert!(
            fila_resumen.contains("2 subagents running (working 1, input-required 1)"),
            "el resumen se recorta a 100 columnas: {fila_resumen:?}"
        );
    }

    #[test]
    fn header_has_no_parentheses_without_subagents() {
        let mut app = App::new();
        let mut term = Terminal::new(TestBackend::new(100, 3)).unwrap();
        term.draw(|f| draw_workspace_header(f, f.area(), &app))
            .unwrap();
        let sin_subagentes: String = term
            .backend()
            .buffer()
            .content
            .iter()
            .map(|celda| celda.symbol())
            .collect();
        assert!(
            !sin_subagentes.contains('('),
            "hay paréntesis sin subagentes: {sin_subagentes}"
        );

        // Solo terminados: el resumen en curso es `0 subagents running`, sin paréntesis.
        app.session_id = Some(codeloom_core::session::SessionId(1));
        app.subagents.insert(
            9,
            codeloom_core::session::SubagentDescriptor {
                parent: codeloom_core::session::SessionId(1),
                child: codeloom_core::session::SessionId(9),
                job_id: "subagent-9".into(),
                label: "review".into(),
                status: TaskStatus::new(TaskState::Failed),
                artifacts: Vec::new(),
                notice_delivered: false,
                agent: None,
                agent_origin: None,
            },
        );
        let mut term = Terminal::new(TestBackend::new(100, 3)).unwrap();
        term.draw(|f| draw_workspace_header(f, f.area(), &app))
            .unwrap();
        let texto: String = term
            .backend()
            .buffer()
            .content
            .iter()
            .map(|celda| celda.symbol())
            .collect();
        // El paréntesis que sigue es el de los terminados; el cero en curso no lleva ninguno.
        assert!(
            texto.contains("0 subagents running (subagents: failed 1)"),
            "falta el resumen: {texto}"
        );
        assert!(
            !texto.contains("0 subagents running (working")
                && !texto.contains("0 subagents running (input"),
            "el cero no debe nombrar estados: {texto}"
        );
    }

    #[test]
    fn workspace_header_marks_errors_instead_of_ready() {
        let mut app = App::new();
        app.status = "error: provider unavailable".into();
        assert!(buffer_of(&mut app, 80, 20).contains("ERROR"));
    }

    #[test]
    fn prompt_queue_previews_the_selected_full_prompt() {
        let mut app = App::new();
        app.pending_prompts
            .push_back("Review the changed files\nThen summarize any risks".into());
        app.queued = 1;
        app.queue_open = true;
        let content = buffer_of(&mut app, 80, 20);
        assert!(content.contains("Preview"));
        assert!(content.contains("Then summarize any risks"));
    }

    #[test]
    fn help_overlay_shows_supported_shortcuts() {
        let mut app = App::new();
        app.help_open = true;
        let content = buffer_of(&mut app, 80, 24);
        assert!(content.contains("Keyboard shortcuts"));
        assert!(content.contains("Ctrl+J"));
        assert!(content.contains("Ctrl+B"));
    }

    #[test]
    fn agents_overlay_shows_the_list_and_the_prompt_of_the_selected_agent() {
        let proyecto = tempfile::tempdir().unwrap();
        std::fs::write(
            proyecto.path().join("front.md"),
            "---
name: front
description: Interfaz
---

Cuerpo de front.
",
        )
        .unwrap();
        let registro = codeloom_core::registry::AgentRegistry::new(vec![
            Box::new(codeloom_core::registry::BuiltInSource),
            Box::new(codeloom_core::registry::FileSource::new(
                proyecto.path().to_path_buf(),
                codeloom_core::registry::ProfileLevel::Project,
            )),
        ]);
        let mut app = App::new();
        app.set_agent_registry(std::sync::Arc::new(registro));
        app.show_agents();
        let content = buffer_of(&mut app, 100, 30);
        assert!(content.contains("Agents"), "{content}");
        assert!(content.contains("default [usable] built-in"), "{content}");
        assert!(
            content.contains("front [awaiting approval] project:"),
            "{content}"
        );

        app.agents_move(1);
        let content = buffer_of(&mut app, 100, 30);
        assert!(content.contains("Prompt of `front`"), "{content}");
        assert!(content.contains("Cuerpo de front."), "{content}");
        assert!(content.contains("a approve"), "{content}");
    }

    #[test]
    fn help_overlay_lists_subagent_navigation() {
        let mut app = App::new();
        app.help_open = true;
        let content = buffer_of(&mut app, 80, 24);
        assert!(content.contains("Alt+←"), "falta Alt+←: {content}");
        assert!(content.contains("/parent"), "falta /parent: {content}");
        assert!(content.contains("↑/↓"), "falta ↑/↓ del rail: {content}");
        assert!(
            content.contains("Press F1 or Esc to close"),
            "la ayuda no cabe con las líneas nuevas: {content}"
        );
    }

    #[test]
    fn hides_sidebar_when_narrow() {
        let mut app = App::with_context("/proj", "m");
        let content = buffer_of(&mut app, 60, 20);
        assert!(!content.contains("subagentes"));
    }

    #[test]
    fn shows_sidebar_when_wide() {
        let mut app = App::with_context("/proj", "mistral-nemo");
        let content = buffer_of(&mut app, 120, 40);
        assert!(content.contains("mistral-nemo"));
        assert!(content.contains("0 subagents running"));
        // Sin subagentes en curso el resumen es `0 subagents running`, sin paréntesis que ocultar.
        assert!(!content.contains("subagents running ("), "{content}");
        assert!(content.contains("CodeLoom"));
    }

    #[test]
    fn shows_modified_files_when_wide() {
        let mut app = App::with_context("/proj", "m");
        app.apply(codeloom_core::event::LiveEvent::FileDiff {
            path: "src/main.rs".into(),
            added: 3,
            removed: 1,
        });
        let content = buffer_of(&mut app, 120, 40);
        assert!(content.contains("ficheros modificados"));
        assert!(content.contains("src/main.rs"));
    }

    #[test]
    fn session_picker_renders_only_matching_previews() {
        let mut app = App::new();
        app.set_sessions(vec![
            codeloom_core::session::SessionInfo {
                id: codeloom_core::session::SessionId(1),
                preview: "Build frontend".into(),
                message_count: 2,
            },
            codeloom_core::session::SessionInfo {
                id: codeloom_core::session::SessionId(2),
                preview: "Fix provider config".into(),
                message_count: 4,
            },
        ]);
        app.session_id = Some(codeloom_core::session::SessionId(2));
        app.session_searching = true;
        app.session_query = "provider".into();
        let content = buffer_of(&mut app, 80, 24);
        assert!(content.contains("Fix provider config"));
        assert!(content.contains("active"));
        assert!(content.contains("1 matches"));
        assert!(!content.contains("Build frontend"));
    }

    #[test]
    fn session_picker_keeps_selected_row_visible_when_list_overflows() {
        let mut app = App::new();
        app.set_sessions(
            (0..8)
                .map(|index| codeloom_core::session::SessionInfo {
                    id: codeloom_core::session::SessionId(index),
                    preview: format!("Session {index}"),
                    message_count: index as usize,
                })
                .collect(),
        );
        app.session_selected = 7;
        let content = buffer_of(&mut app, 80, 12);
        assert!(content.contains("Session 7"));
        assert!(content.contains("Ctrl+F filter"));
    }

    #[test]
    fn ctrl_b_sidebar_is_visible_at_medium_width() {
        let mut app = App::with_context("/proj", "m");
        app.sidebar_expanded = true;
        let content = buffer_of(&mut app, 90, 30);
        assert!(content.contains("PROJECT"));
        assert!(content.contains("0 subagents running"));
        assert!(content.contains("Conversation"));
    }

    #[test]
    fn expanded_tool_output_uses_available_transcript_height() {
        let mut tool = batten::state::ToolItem::new("call-1", "view");
        tool.set_result(false);
        tool.set_output(
            (0..20)
                .map(|i| format!("line-{i}"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        tool.expanded = true;
        let mut app = App::new();
        app.tools.push_back(tool);
        let content = buffer_of(&mut app, 120, 40);
        assert!(content.contains("line-19"));
    }

    #[test]
    fn live_tool_card_separates_status_arguments_and_output() {
        let mut tool = batten::state::ToolItem::new("call-1", "view");
        tool.set_summary(r#"{"path":"src/main.rs"}"#);
        tool.set_result(false);
        tool.set_output("fn main() {}");
        let mut app = App::new();
        app.tools.push_back(tool);
        let content = buffer_of(&mut app, 90, 20);
        assert!(content.contains("DONE"));
        assert!(content.contains("IN"));
        assert!(content.contains("OUT"));
        assert!(content.contains("fn main()"));
        assert!(content.contains("src/main.rs"));
    }

    #[test]
    fn crush_shows_inline_tool_once_without_fixed_card() {
        let mut app = App::new();
        app.theme = batten::widgets::Theme::preset(batten::widgets::ThemePreset::Crush);
        app.apply(codeloom_core::event::LiveEvent::ToolStarted {
            call_id: "c1".into(),
            name: "View".into(),
        });
        // Por debajo de 80 columnas no hay rail, que también lista la tool.
        let content = buffer_of(&mut app, 70, 20);
        assert_eq!(content.matches("● View").count(), 1);
        assert!(!content.contains("RUNNING"));
    }

    #[test]
    fn crush_composer_keeps_cursor_inside_the_screen_when_wrapping() {
        let mut app = App::new();
        app.theme = batten::widgets::Theme::preset(batten::widgets::ThemePreset::Crush);
        app.composer.set_text(&"x".repeat(100));
        let backend = TestBackend::new(40, 20);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f, &mut app)).unwrap();
        let cursor = term.backend_mut().get_cursor_position().unwrap();
        assert!(cursor.x < 40);
        assert!(cursor.y > 0 && cursor.y < 20);
    }

    #[test]
    fn composer_grows_for_multiline_input_and_keeps_cursor_visible_when_limited() {
        let mut app = App::new();
        app.composer.set_text("first\nsecond\nthird");
        let backend = TestBackend::new(60, 20);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f, &mut app)).unwrap();
        let content = term
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        let cursor = term.backend_mut().get_cursor_position().unwrap();
        assert!(content.contains("first"));
        assert!(content.contains("second"));
        assert!(content.contains("third"));
        assert!(cursor.y > 0);

        app.composer.set_text("one\ntwo\nthree\nfour\nfive");
        let backend = TestBackend::new(60, 12);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f, &mut app)).unwrap();
        let content = term
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        let cursor = term.backend_mut().get_cursor_position().unwrap();
        assert!(content.contains("three"));
        assert!(content.contains("four"));
        assert!(content.contains("five"));
        assert!(cursor.y > 0);
    }

    #[test]
    fn slash_menu_renders_filters_and_respects_overlays_and_small_sizes() {
        let mut app = App::new();
        app.composer.set_text("/");
        let content = buffer_of(&mut app, 100, 30);
        assert!(content.contains("/help"));
        assert!(content.contains("/sessions"));
        assert!(content.contains("Tab complete"));
        app.composer.set_text("/to");
        let content = buffer_of(&mut app, 100, 30);
        assert!(content.contains("/tools"));
        assert!(!content.contains("/sessions"));
        for (width, height) in [(0, 0), (1, 1), (20, 5), (40, 10), (120, 40)] {
            buffer_of(&mut app, width, height);
        }
        app.composer.set_text("/unknown");
        assert!(buffer_of(&mut app, 100, 30).contains("Unknown command"));
        app.composer.set_text("/new");
        app.turn_in_flight = true;
        crate::run::handle_key(
            &mut app,
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        );
        assert!(buffer_of(&mut app, 100, 30).contains("Wait for the active turn"));
        app.help_open = true;
        let content = buffer_of(&mut app, 100, 30);
        assert!(!content.contains("Tab complete"));
        assert!(content.contains("Press F1 or Esc to close"));
    }

    #[test]
    fn tool_inspector_shows_status_arguments_and_scrolls_full_output() {
        let mut app = App::new();
        app.tools_open = true;
        assert!(buffer_of(&mut app, 100, 30).contains("No tool calls yet"));
        let mut tool = batten::state::ToolItem::new("call", "view");
        tool.set_summary("{\"path\":\"sample.rs\"}");
        tool.set_result(true);
        tool.set_output(
            (0..40)
                .map(|index| format!("line-{index:02}"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        app.tools.push_back(tool);
        let top = buffer_of(&mut app, 100, 30);
        assert!(top.contains("Error"));
        assert!(top.contains("sample.rs"));
        assert!(top.contains("line-00"));
        assert!(!top.contains("line-39"));
        app.tool_scroll = (u16::MAX, u16::MAX);
        let bottom = buffer_of(&mut app, 100, 30);
        assert!(bottom.contains("line-39"));
        assert_eq!(app.tool_scroll.1, 0);
        for (width, height) in [(0, 0), (1, 1), (30, 8), (60, 20)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| render(frame, &mut app)).unwrap();
            assert!(!terminal.backend().cursor_visible());
        }
        assert!(buffer_of(&mut app, 30, 8).contains("enlarge"));
        app.tools[0].set_output(format!("{}THE-END", "x".repeat(200)));
        app.tool_scroll = (0, u16::MAX);
        assert!(buffer_of(&mut app, 100, 30).contains("THE-END"));
    }

    #[test]
    fn queue_shows_pause_controls_and_edits_with_its_own_cursor() {
        let mut app = App::new();
        app.queue_open = true;
        app.queue_paused = true;
        app.pending_prompts.push_back("original".into());
        let content = buffer_of(&mut app, 100, 30);
        assert!(content.contains("paused"));
        assert!(content.contains("Enter edit"));
        assert!(content.contains("Ctrl+X"));
        app.queue_editing = true;
        app.queue_editor.set_text("edited\nsecond line");
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        assert!(terminal.backend().cursor_visible());
        let content = buffer_of(&mut app, 100, 30);
        assert!(content.contains("second line"));
        assert!(content.contains("Esc discard edit"));
    }

    #[test]
    fn overlays_render_across_terminal_sizes_and_hide_the_composer_cursor() {
        for overlay in 0..6 {
            let mut app = App::new();
            match overlay {
                0 => app.session_loading = true,
                1 => app.session_picker_open = true,
                2 => app.queue_open = true,
                3 => app.help_open = true,
                4 => {
                    app.set_provider_settings(crate::app::ProviderSettings {
                        kind: codeloom_core::config::ProviderKind::OpenAiCompat,
                        model: "test-model".into(),
                        base_url: "https://example.test/v1".into(),
                        api_key_env: "API_KEY".into(),
                        effort: None,
                    });
                    app.open_settings();
                }
                _ => {
                    let (tx, _rx) = tokio::sync::oneshot::channel();
                    app.enqueue_permission(
                        codeloom_core::permission::PermissionRequest {
                            tool: "bash".into(),
                            action: "run".into(),
                            detail: "cargo test".into(),
                        },
                        tx,
                    );
                }
            }
            for (width, height) in [(0, 0), (1, 1), (10, 3), (30, 8), (60, 20), (120, 40)] {
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                terminal.draw(|frame| render(frame, &mut app)).unwrap();
                assert!(
                    !terminal.backend().cursor_visible(),
                    "overlay {overlay} at {width}x{height}"
                );
            }
            if (1..=4).contains(&overlay) {
                assert!(buffer_of(&mut app, 20, 8).contains("enlarge"));
            }
        }
        let mut app = App::new();
        app.help_open = true;
        let help = buffer_of(&mut app, 80, 24);
        assert!(help.contains("Shift+Tab"));
        assert!(help.contains("Press F1 or Esc to close"));
    }

    #[test]
    fn provider_settings_shows_saving_and_recoverable_error() {
        let mut app = App::new();
        app.set_provider_settings(crate::app::ProviderSettings {
            kind: codeloom_core::config::ProviderKind::OpenAiCompat,
            model: "test-model".into(),
            base_url: "https://example.test/v1".into(),
            api_key_env: "API_KEY".into(),
            effort: None,
        });
        app.open_settings();
        app.settings_saving = true;
        let content = buffer_of(&mut app, 120, 40);
        assert!(content.contains("Applying"));
        assert!(content.contains("API_KEY"));
        app.settings_saved(Err("Cannot write config".into()));
        let content = buffer_of(&mut app, 120, 40);
        assert!(content.contains("Cannot write config"));
        assert!(content.contains("Ctrl+S"));
        let content = buffer_of(&mut app, 30, 8);
        assert!(content.contains("Provider settings"));
        assert!(content.contains("Esc close"));
    }

    #[test]
    fn shows_permission_dialog_on_top() {
        let mut app = App::new();
        let (tx, _rx) = tokio::sync::oneshot::channel();
        app.enqueue_permission(
            codeloom_core::permission::PermissionRequest {
                tool: "bash".into(),
                action: "run".into(),
                detail: r#"{"command":"cargo test"}"#.into(),
            },
            tx,
        );
        let content = buffer_of(&mut app, 120, 40);
        assert!(content.contains("Permiso"));
        assert!(content.contains("cargo test"));
        assert!(content.contains("Denegar"));
    }

    fn ask_question_for_render() -> codeloom_core::question::Question {
        use codeloom_core::question::{Question, QuestionOption};
        let option = |label: &str| QuestionOption {
            label: label.into(),
            description: String::new(),
        };
        Question {
            header: "Stack".into(),
            question: "Which framework?".into(),
            options: vec![option("Alpha"), option("Beta")],
            multi_select: false,
        }
    }

    #[test]
    fn question_dialog_renders_and_hides_the_composer_cursor() {
        let mut app = App::new();
        let (tx, _rx) = tokio::sync::oneshot::channel();
        app.enqueue_questions(vec![ask_question_for_render()], tx);
        let content = buffer_of(&mut app, 120, 40);
        assert!(content.contains("Question 1/1"));
        assert!(content.contains("Which framework?"));
        assert!(content.contains("Alpha"));
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        assert!(!terminal.backend().cursor_visible());
    }

    #[test]
    fn permission_is_drawn_above_a_question() {
        use codeloom_core::permission::PermissionRequest;
        let request = PermissionRequest {
            tool: "bash".into(),
            action: "run".into(),
            detail: r#"{"command":"cargo test"}"#.into(),
        };
        let render_with_question = |open_question: bool| {
            let mut app = App::new();
            if open_question {
                let (tx, _rx) = tokio::sync::oneshot::channel();
                app.enqueue_questions(vec![ask_question_for_render()], tx);
            }
            let (ptx, _prx) = tokio::sync::oneshot::channel();
            app.enqueue_permission(request.clone(), ptx);
            let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
            terminal.draw(|frame| render(frame, &mut app)).unwrap();
            terminal.backend().buffer().clone()
        };
        let alone = render_with_question(false);
        let stacked = render_with_question(true);
        let text: String = stacked.content.iter().map(|c| c.symbol()).collect();
        assert!(text.contains("Permiso"));
        assert!(text.contains("cargo test"));
        assert!(
            text.contains("Question 1/1"),
            "la pregunta debe seguir visible fuera del permiso"
        );
        // Las celdas del permiso no pueden mostrar nada de la pregunta: se pintó encima.
        let dialog = crate::permission_flow::dialog_for(&request);
        let rect = batten::widgets::permission::area(Rect::new(0, 0, 120, 40), &dialog);
        for y in rect.top()..rect.bottom() {
            for x in rect.left()..rect.right() {
                let i = usize::from(y) * 120 + usize::from(x);
                assert_eq!(
                    stacked.content[i].symbol(),
                    alone.content[i].symbol(),
                    "celda ({x},{y}) pintada por debajo del permiso"
                );
            }
        }
    }

    fn chatty_app() -> App {
        let mut app = App::new();
        for i in 0..60 {
            app.push_user(&format!("msg-{i:02}"));
        }
        app
    }

    #[test]
    fn newest_messages_are_visible_when_overflowing() {
        let mut app = chatty_app();
        let content = buffer_of(&mut app, 60, 20);
        assert!(content.contains("msg-59"));
        assert!(!content.contains("msg-00"));
    }

    #[test]
    fn page_up_scrolls_back_and_stops_following() {
        let mut app = chatty_app();
        buffer_of(&mut app, 60, 20);
        crate::run::handle_key(
            &mut app,
            crossterm::event::KeyCode::PageUp,
            crossterm::event::KeyModifiers::NONE,
        );
        let content = buffer_of(&mut app, 60, 20);
        assert!(!app.viewport.is_following());
        assert!(!content.contains("msg-59"));
    }

    /// Aplicación con el look Crush (layout `ContextRail`, chrome `Flat`).
    fn crush_ctx() -> App {
        let mut app = App::with_context("/proj", "mistral-nemo");
        app.theme_preset = batten::widgets::ThemePreset::Crush;
        app.theme = batten::widgets::Theme::preset(batten::widgets::ThemePreset::Crush);
        app
    }

    /// Job de prueba con el dueño y el tipo indicados.
    fn job_snapshot(
        id: &str,
        label: &str,
        kind: &str,
        status: codeloom_core::job::JobStatus,
        owner: codeloom_core::session::SessionId,
    ) -> codeloom_core::job::JobSnapshot {
        codeloom_core::job::JobSnapshot {
            id: codeloom_core::job::JobId(id.into()),
            kind: kind.into(),
            label: label.into(),
            owner,
            status,
            progress: None,
            output: String::new(),
            output_start: 0,
            output_end: 0,
            detail: None,
            result: None,
        }
    }

    /// Aplicación Crush con ficheros, herramienta, job y subagente de la sesión 1.
    fn loaded_crush_app() -> App {
        use codeloom_core::event::LiveEvent;
        use codeloom_core::job::JobStatus;
        use codeloom_core::session::{SessionId, SubagentDescriptor};
        let mut app = crush_ctx();
        app.session_id = Some(SessionId(1));
        app.apply(LiveEvent::FileDiff {
            path: "src/main.rs".into(),
            added: 3,
            removed: 1,
        });
        app.apply(LiveEvent::ToolStarted {
            call_id: "c1".into(),
            name: "View".into(),
        });
        app.apply(LiveEvent::JobChanged(job_snapshot(
            "build-1",
            "build",
            "shell",
            JobStatus::Running,
            SessionId(1),
        )));
        app.apply(LiveEvent::SubagentChanged(SubagentDescriptor {
            parent: SessionId(1),
            child: SessionId(9),
            job_id: "subagent-1".into(),
            label: "review".into(),
            status: TaskStatus::new(TaskState::Working),
            artifacts: Vec::new(),
            notice_delivered: false,
            agent: None,
            agent_origin: None,
        }));
        app
    }

    #[test]
    fn crush_layout_has_only_a_right_rail_when_wide() {
        let mut app = crush_ctx();
        let content = buffer_of(&mut app, 120, 40);
        assert!(content.contains("MODEL"));
        assert!(content.contains("mistral-nemo"));
        assert!(!content.contains("PROJECT")); // secciones del sidebar izquierdo
        assert!(!content.contains("FLOCK"));
        assert!(!content.contains("Token usage")); // panel de métricas
        assert!(!content.contains("Conversation")); // panel del chat
    }

    #[test]
    fn crush_without_width_for_the_rail_uses_the_whole_screen() {
        let mut app = crush_ctx();
        let content = buffer_of(&mut app, 70, 24);
        assert!(!content.contains("MODEL"));
        assert!(content.contains("CODELOOM"));
    }

    #[test]
    fn toggle_sidebar_follows_the_look() {
        let mut app = crush_ctx();
        assert!(buffer_of(&mut app, 120, 30).contains("MODEL"));
        app.toggle_sidebar();
        assert!(app.rail_hidden && !app.sidebar_expanded);
        assert!(!buffer_of(&mut app, 120, 30).contains("MODEL"));
        app.toggle_sidebar();
        assert!(buffer_of(&mut app, 120, 30).contains("MODEL"));
        let mut classic = App::with_context("/proj", "m");
        classic.toggle_sidebar();
        assert!(classic.sidebar_expanded && !classic.rail_hidden);
    }

    #[test]
    fn crush_header_is_one_flat_line() {
        let mut app = crush_ctx();
        let content = buffer_of(&mut app, 120, 30);
        // `buffer_of` no inserta saltos de línea: la primera fila son las primeras 120 celdas.
        let first: String = content.chars().take(120).collect();
        assert!(first.contains("CODELOOM"));
        assert!(first.contains("/proj"));
        assert!(first.contains("ready"));
        assert!(!content.contains("WORKSPACE"));
    }

    #[test]
    fn crush_footer_shows_short_key_hints() {
        let mut app = crush_ctx();
        let content = buffer_of(&mut app, 120, 30);
        assert!(content.contains("enter send"));
        assert!(content.contains("ctrl+j newline"));
    }

    #[test]
    fn rail_lists_files_tools_jobs_and_subagents() {
        let mut app = loaded_crush_app();
        let content = buffer_of(&mut app, 120, 40);
        for expected in [
            "FILES",
            "src/main.rs",
            "TOOLS",
            "View",
            "JOBS",
            "build",
            "SUBAGENTS",
            "review",
        ] {
            assert!(content.contains(expected), "falta {expected:?} en el rail");
        }
    }

    #[test]
    fn rail_caps_long_lists() {
        let mut app = crush_ctx();
        for i in 0..40 {
            app.apply(codeloom_core::event::LiveEvent::FileDiff {
                path: format!("src/file{i:02}.rs"),
                added: 1,
                removed: 0,
            });
        }
        let content = buffer_of(&mut app, 120, 40);
        assert!(content.contains("… +32 more"));
        assert!(!content.contains("file20.rs"));
    }

    #[test]
    fn rail_does_not_panic_on_tiny_heights() {
        let mut app = loaded_crush_app();
        for (width, height) in [(120, 1), (120, 2), (120, 3), (80, 4), (200, 5)] {
            let _ = buffer_of(&mut app, width, height);
        }
    }

    /// Tarea de prueba para el rail.
    fn todo_item(
        content: &str,
        status: codeloom_core::todo::TodoStatus,
    ) -> codeloom_core::todo::TodoItem {
        codeloom_core::todo::TodoItem {
            content: content.into(),
            status,
        }
    }

    #[test]
    fn rail_lists_todos_with_progress() {
        use codeloom_core::event::LiveEvent;
        use codeloom_core::session::SessionId;
        use codeloom_core::todo::TodoStatus::{Completed, Pending};
        let mut app = crush_ctx();
        app.session_id = Some(SessionId(1));
        app.apply(LiveEvent::TodosChanged {
            session: SessionId(1),
            items: vec![
                todo_item("Escribir el parser", Completed),
                todo_item("Pintar el rail", Pending),
            ],
        });
        let content = buffer_of(&mut app, 120, 40);
        assert!(content.contains("TODOS"));
        assert!(content.contains("1/2"));
        assert!(content.contains("Pintar el rail"));
    }

    #[test]
    fn rail_caps_long_todo_lists() {
        use codeloom_core::event::LiveEvent;
        use codeloom_core::session::SessionId;
        use codeloom_core::todo::TodoStatus::Pending;
        let mut app = crush_ctx();
        app.session_id = Some(SessionId(1));
        let items = (1..=20)
            .map(|n| todo_item(&format!("tarea {n:02}"), Pending))
            .collect();
        app.apply(LiveEvent::TodosChanged {
            session: SessionId(1),
            items,
        });
        let content = buffer_of(&mut app, 120, 40);
        assert!(content.contains("… +12 more"));
        assert!(!content.contains("tarea 20"));
    }

    /// Renderiza y devuelve el terminal, para inspeccionar celdas concretas.
    fn terminal_of(app: &mut App, w: u16, h: u16) -> Terminal<TestBackend> {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| render(f, app)).unwrap();
        term
    }

    /// Rectángulo registrado en el último fotograma para una ventana.
    fn rect_of(app: &App, pane: Focus) -> Rect {
        app.pane_rects
            .iter()
            .find(|(p, _)| *p == pane)
            .map(|(_, r)| *r)
            .unwrap_or_else(|| panic!("falta la ventana {pane:?}"))
    }

    #[test]
    fn render_records_pane_rects_classic_wide() {
        let mut app = App::new();
        let _ = terminal_of(&mut app, 120, 40);
        let panes: Vec<Focus> = app.pane_rects.iter().map(|(p, _)| *p).collect();
        for pane in [
            Focus::Sidebar,
            Focus::Transcript,
            Focus::Agent,
            Focus::Activity,
            Focus::Composer,
        ] {
            assert!(panes.contains(&pane), "falta {pane:?}");
        }
        assert!(!panes.contains(&Focus::Rail));
        assert_eq!(app.composer_area, Some(rect_of(&app, Focus::Composer)));
    }

    #[test]
    fn render_records_pane_rects_crush_with_and_without_rail() {
        let mut app = crush_ctx();
        let _ = terminal_of(&mut app, 120, 40);
        let panes: Vec<Focus> = app.pane_rects.iter().map(|(p, _)| *p).collect();
        assert!(panes.contains(&Focus::Rail));
        assert!(panes.contains(&Focus::Transcript));
        assert!(panes.contains(&Focus::Composer));
        assert!(!panes.contains(&Focus::Sidebar));
        assert!(!panes.contains(&Focus::Agent) && !panes.contains(&Focus::Activity));
        // Sin ancho para el rail, el mapa se reescribe y ya no lo lista.
        let _ = terminal_of(&mut app, 70, 24);
        let panes: Vec<Focus> = app.pane_rects.iter().map(|(p, _)| *p).collect();
        assert!(!panes.contains(&Focus::Rail));
        assert!(panes.contains(&Focus::Transcript) && panes.contains(&Focus::Composer));
    }

    #[test]
    fn focused_panel_border_uses_the_accent_style() {
        let mut app = App::new();
        let _ = terminal_of(&mut app, 120, 40);
        let agent = rect_of(&app, Focus::Agent);
        let edge = (agent.x, agent.y + 1);
        let accent = app.theme.styles.accent.fg.expect("el acento tiene color");

        let unfocused = terminal_of(&mut app, 120, 40);
        assert_ne!(unfocused.backend().buffer()[edge].fg, accent);

        app.focus = Focus::Agent;
        let focused = terminal_of(&mut app, 120, 40);
        assert_eq!(focused.backend().buffer()[edge].fg, accent);
    }

    #[test]
    fn focused_rail_marks_its_edge_in_crush() {
        let mut app = crush_ctx();
        let _ = terminal_of(&mut app, 120, 40);
        let rail = rect_of(&app, Focus::Rail);
        let accent = app.theme.styles.accent.fg.expect("el acento tiene color");
        app.focus = Focus::Rail;
        let term = terminal_of(&mut app, 120, 40);
        assert_eq!(term.backend().buffer()[(rail.x, rail.y + 1)].fg, accent);
    }

    #[test]
    fn selected_transcript_cells_carry_the_selection_style() {
        use batten::state::{CellPos, TextSelection};
        let mut app = App::new();
        for i in 0..40 {
            app.push_user(&format!("linea {i:02}"));
        }
        let _ = terminal_of(&mut app, 100, 30);
        let area = app.transcript_area.expect("el transcript fija su área");
        let last = app.layout.total_rows() - 1;
        let mut sel = TextSelection::new(CellPos { row: 0, col: 0 });
        sel.set_head(CellPos { row: last, col: 0 });
        app.text_sel = Some(sel);
        app.text_sel_width = app.layout.width();
        let term = terminal_of(&mut app, 100, 30);
        let sel_bg = app
            .theme
            .styles
            .selection
            .bg
            .expect("la selección tiene fondo");
        let cell = &term.backend().buffer()[(area.x, area.y)];
        assert_eq!(cell.bg, sel_bg);
    }

    #[test]
    fn transcript_focus_no_longer_paints_the_full_margin_filete() {
        let mut app = crush_ctx();
        app.push_user("hola");
        let _ = terminal_of(&mut app, 120, 40);
        app.focus = Focus::Transcript;
        let term = terminal_of(&mut app, 120, 40);
        let area = app.transcript_area.expect("el transcript fija su área");
        let margin = area.x - 1;
        // La fila 0 es el bloque: lleva su línea por rol, no el filete de foco.
        assert_eq!(term.backend().buffer()[(margin, area.y)].symbol(), "│");
        // Una fila por debajo del contenido no pertenece a ningún bloque: sin filete.
        let below = area.y + area.height - 1;
        assert_eq!(term.backend().buffer()[(margin, below)].symbol(), " ");
        for y in area.y..area.bottom() {
            assert_ne!(term.backend().buffer()[(margin, y)].symbol(), "▎");
        }
    }

    /// Filas `[inicio, fin)` de un bloque, según el diseño del último fotograma.
    fn block_rows(app: &App, entry: usize) -> (usize, usize) {
        let first = app.transcript.entry_start(entry);
        let start = app.layout.row_of(first);
        let end = app
            .layout
            .row_of(first + app.transcript.entry_lines(entry).len());
        (start, end)
    }

    #[test]
    fn marker_is_drawn_for_the_active_block_in_flat_chrome() {
        let mut app = crush_ctx();
        for i in 0..3 {
            app.push_user(&format!("bloque {i}"));
        }
        let _ = terminal_of(&mut app, 120, 40);
        app.focus = Focus::Transcript;
        app.block = Some(1);
        let term = terminal_of(&mut app, 120, 40);
        let area = app.transcript_area.expect("el transcript fija su área");
        let margin = area.x - 1;
        let top = app.viewport.top();
        let (start, end) = block_rows(&app, 1);
        assert!(start > 0, "el bloque 0 queda antes, con su propio filete");
        for row in start..end {
            let y = area.y + (row - top) as u16;
            assert_eq!(
                term.backend().buffer()[(margin, y)].symbol(),
                "▌",
                "la fila {row} del bloque activo lleva marcador grueso"
            );
        }
        assert_eq!(
            term.backend().buffer()[(margin, area.y)].symbol(),
            "│",
            "fuera del bloque sigue su línea por rol, sin filete de foco"
        );
    }

    #[test]
    fn marker_not_drawn_when_transcript_unfocused() {
        let mut app = crush_ctx();
        for i in 0..3 {
            app.push_user(&format!("bloque {i}"));
        }
        let _ = terminal_of(&mut app, 120, 40);
        app.focus = Focus::Composer;
        app.block = Some(1);
        let term = terminal_of(&mut app, 120, 40);
        let area = app.transcript_area.expect("el transcript fija su área");
        let margin = area.x - 1;
        let (start, end) = block_rows(&app, 1);
        for row in start..end {
            let y = area.y + (row - app.viewport.top()) as u16;
            assert_ne!(term.backend().buffer()[(margin, y)].symbol(), "█");
        }
    }

    #[test]
    fn marker_recolors_first_cell_without_margin() {
        let mut app = App::new();
        app.push_user("primera");
        app.push_user("segunda");
        let _ = terminal_of(&mut app, 100, 30);
        assert_ne!(app.theme.look.chrome, ChromeStyle::Flat);
        app.focus = Focus::Transcript;
        app.block = Some(1);
        let term = terminal_of(&mut app, 100, 30);
        let area = app.transcript_area.expect("el transcript fija su área");
        let (start, _) = block_rows(&app, 1);
        let y = area.y + (start - app.viewport.top()) as u16;
        let accent = app.theme.styles.accent.fg.expect("el acento tiene color");
        assert_eq!(term.backend().buffer()[(area.x, y)].fg, accent);
    }

    /// Celda de la columna de margen en la fila global `row` del transcript.
    fn margin_cell(term: &Terminal<TestBackend>, app: &App, row: usize) -> ratatui::buffer::Cell {
        let area = app.transcript_area.expect("el transcript fija su área");
        let y = area.y + (row - app.viewport.top()) as u16;
        term.backend().buffer()[(area.x - 1, y)].clone()
    }

    #[test]
    fn only_user_blocks_have_a_thin_rail_when_inactive() {
        use batten::state::{Format, LineKind};
        let mut app = crush_ctx();
        app.push_user("pregunta");
        app.push_assistant("respuesta");
        app.transcript
            .push_entry(LineKind::Thinking, Format::Plain, "pensando");
        app.transcript
            .push_entry(LineKind::Plan, Format::Plain, "plan");
        app.transcript
            .push_entry(LineKind::ToolCall, Format::Plain, "herramienta");
        app.transcript
            .push_entry(LineKind::Error, Format::Plain, "fallo");
        app.push_user("otra pregunta");
        let term = terminal_of(&mut app, 120, 40);
        assert_eq!(app.transcript.entry_count(), 7);
        let accent = app.theme.styles.accent.fg.expect("acento con color");
        for entry in [0, 6] {
            let (start, end) = block_rows(&app, entry);
            assert!(end > start, "el bloque {entry} ocupa filas");
            for row in start..end {
                let cell = margin_cell(&term, &app, row);
                assert_eq!(cell.symbol(), "│", "el usuario {entry} lleva línea fina");
                assert_eq!(cell.fg, accent);
            }
        }
        for entry in 1..6 {
            let (start, end) = block_rows(&app, entry);
            assert!(end > start, "el bloque {entry} ocupa filas");
            for row in start..end {
                assert_eq!(
                    margin_cell(&term, &app, row).symbol(),
                    " ",
                    "el bloque {entry} inactivo no lleva línea"
                );
            }
        }
    }

    #[test]
    fn blocks_without_role_have_no_rail() {
        let mut app = crush_ctx();
        app.transcript.push_entry(
            batten::state::LineKind::Banner,
            batten::state::Format::Plain,
            "banner",
        );
        app.push_user("hola");
        app.transcript.push_entry(
            batten::state::LineKind::System,
            batten::state::Format::Plain,
            "sistema",
        );
        let term = terminal_of(&mut app, 120, 40);
        for entry in [0, 2] {
            let (start, end) = block_rows(&app, entry);
            for row in start..end {
                assert_eq!(margin_cell(&term, &app, row).symbol(), " ");
            }
        }
        let (start, end) = block_rows(&app, 1);
        assert!(end > start);
        for row in start..end {
            assert_eq!(margin_cell(&term, &app, row).symbol(), "│");
        }
    }

    #[test]
    fn active_assistant_block_gets_thick_rail_with_soft_success_style() {
        use batten::state::{Format, LineKind};
        let mut app = crush_ctx();
        app.push_user("a");
        app.transcript
            .push_entry(LineKind::Assistant, Format::Plain, "b");
        app.push_user("c");
        let _ = terminal_of(&mut app, 120, 40);
        app.focus = Focus::Transcript;
        app.block = Some(1);
        let term = terminal_of(&mut app, 120, 40);
        let success = app.theme.styles.success.fg.expect("el éxito tiene color");
        let accent = app.theme.styles.accent.fg.expect("acento con color");
        assert_ne!(success, accent, "el éxito no se confunde con el acento");
        let (start, end) = block_rows(&app, 1);
        assert!(end > start, "el asistente ocupa filas");
        for row in start..end {
            let cell = margin_cell(&term, &app, row);
            assert_eq!(cell.symbol(), "▌", "la fila {row} del activo es gruesa");
            assert_eq!(cell.fg, success, "la fila {row} del activo es de éxito");
        }
    }

    #[test]
    fn active_user_block_gets_thick_accent_rail() {
        let mut app = crush_ctx();
        app.push_user("a");
        app.push_assistant("b");
        app.push_user("c");
        let _ = terminal_of(&mut app, 120, 40);
        app.focus = Focus::Transcript;
        app.block = Some(0);
        let term = terminal_of(&mut app, 120, 40);
        let accent = app.theme.styles.accent.fg.expect("acento con color");
        let (start, end) = block_rows(&app, 0);
        for row in start..end {
            let cell = margin_cell(&term, &app, row);
            assert_eq!(cell.symbol(), "▌", "la fila {row} del activo es gruesa");
            assert_eq!(cell.fg, accent, "la fila {row} del activo es de acento");
        }
        // El usuario inactivo conserva su línea fina; el asistente, nada.
        let (start, end) = block_rows(&app, 2);
        for row in start..end {
            assert_eq!(margin_cell(&term, &app, row).symbol(), "│");
        }
        let (start, end) = block_rows(&app, 1);
        for row in start..end {
            assert_eq!(margin_cell(&term, &app, row).symbol(), " ");
        }
    }

    #[test]
    fn rail_spans_every_row_of_a_wrapped_block() {
        let mut app = crush_ctx();
        app.push_user(&"palabra ".repeat(60));
        app.push_user("corto");
        let term = terminal_of(&mut app, 60, 30);
        let (start, end) = block_rows(&app, 0);
        assert!(end - start > 1, "el bloque ocupa varias filas");
        for row in start..end {
            assert_eq!(margin_cell(&term, &app, row).symbol(), "│");
        }
    }

    #[test]
    fn no_rails_in_classic_chrome() {
        let mut app = App::new();
        app.push_user("primera");
        app.push_assistant("segunda");
        let term = terminal_of(&mut app, 100, 30);
        assert_ne!(app.theme.look.chrome, ChromeStyle::Flat);
        let border = app.theme.styles.border.fg.expect("el borde tiene color");
        for entry in 0..2 {
            let (start, end) = block_rows(&app, entry);
            for row in start..end {
                let cell = margin_cell(&term, &app, row);
                assert_eq!(cell.fg, border, "el margen clásico es el borde del panel");
            }
        }
    }

    #[test]
    fn rails_follow_the_viewport_scroll() {
        let mut app = crush_ctx();
        for i in 0..20 {
            if i % 2 == 0 {
                app.push_user(&format!("pregunta {i}"));
            } else {
                app.transcript.push_entry(
                    batten::state::LineKind::Assistant,
                    batten::state::Format::Plain,
                    &format!("respuesta {i}"),
                );
            }
        }
        let term = terminal_of(&mut app, 60, 15);
        let user = app.theme.styles.accent.fg.expect("acento con color");
        let (last, _) = block_rows(&app, 19);
        assert_eq!(
            margin_cell(&term, &app, last).symbol(),
            " ",
            "al final se ve el último bloque, asistente, sin línea"
        );
        // Cada bloque de usuario visible lleva su línea en todas sus filas.
        let top = app.viewport.top();
        let bottom = top + 15;
        for entry in (0..20).step_by(2) {
            let (start, end) = block_rows(&app, entry);
            for row in start.max(top)..end.min(bottom) {
                assert_eq!(margin_cell(&term, &app, row).symbol(), "│");
            }
        }
        app.viewport.to_top();
        let term = terminal_of(&mut app, 60, 15);
        assert_eq!(app.viewport.top(), 0);
        let (first, _) = block_rows(&app, 0);
        assert_eq!(
            margin_cell(&term, &app, first).fg,
            user,
            "tras subir, el primer bloque es del usuario"
        );
    }

    #[test]
    fn settings_dialog_marks_changed_fields_unsaved() {
        let mut app = App::new();
        app.set_provider_settings(crate::app::ProviderSettings {
            kind: codeloom_core::config::ProviderKind::OpenAiCompat,
            model: "test-model".into(),
            base_url: "https://example.test/v1".into(),
            api_key_env: "API_KEY".into(),
            effort: None,
        });
        app.open_settings();
        assert!(!buffer_of(&mut app, 120, 40).contains("unsaved"));
        app.provider_settings.as_mut().unwrap().model = "other-model".into();
        let content = buffer_of(&mut app, 120, 40);
        assert_eq!(content.matches("● unsaved").count(), 1);
        assert!(content.contains("other-model"));
        assert!(content.contains("Save and apply (1 change)"));
    }

    #[test]
    fn picker_renders_items_with_detail() {
        use crate::app::SettingsPicker;

        let mut app = App::new();
        app.set_provider_settings(crate::app::ProviderSettings {
            kind: codeloom_core::config::ProviderKind::Anthropic,
            model: "model-a".into(),
            base_url: "https://api.anthropic.com".into(),
            api_key_env: "ANTHROPIC_API_KEY".into(),
            effort: None,
        });
        app.set_catalog(crate::app::picker_sample_catalog());
        app.open_settings();
        app.open_picker(SettingsPicker::Model);
        let content = buffer_of(&mut app, 120, 40);
        assert!(content.contains("Claude Sonnet"));
        assert!(content.contains("ctx 200k · $3/$15"));
        assert!(content.contains("ctx 200k · -"));
    }

    #[test]
    fn settings_dialog_shows_effort_only_for_reasoning_models() {
        let mut app = App::new();
        app.set_catalog(crate::app::reasoning_sample_catalog());
        app.set_provider_settings(crate::app::ProviderSettings {
            kind: codeloom_core::config::ProviderKind::Anthropic,
            model: "claude-think".into(),
            base_url: "https://api.anthropic.com/v1".into(),
            api_key_env: "ANTHROPIC_API_KEY".into(),
            effort: None,
        });
        app.open_settings();
        assert!(buffer_of(&mut app, 120, 40).contains("Effort: default"));

        app.cycle_effort(true);
        let content = buffer_of(&mut app, 120, 40);
        assert!(content.contains("Effort: low"));
        assert_eq!(content.matches("● unsaved").count(), 1);

        app.provider_settings.as_mut().unwrap().model = "claude-plain".into();
        assert!(!buffer_of(&mut app, 120, 40).contains("Effort:"));
    }
}
