use crate::app::Focus;
use crate::app::{App, ProviderSettings, SessionChoice};
use crate::frame::{FrameScheduler, Urgency, MIN_FRAME};
use crate::render::render;
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, Event,
    KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, BeginSynchronizedUpdate, EndSynchronizedUpdate,
    EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

pub type ActiveTurn = Arc<Mutex<Option<CancellationToken>>>;
const ESC_DOUBLE_PRESS: Duration = Duration::from_millis(350);

pub enum SessionCommand {
    Refresh,
    Select(SessionChoice),
}

pub enum SessionUpdate {
    List(Vec<codeloom_core::session::SessionInfo>),
    Loaded(
        codeloom_core::session::SessionId,
        Vec<codeloom_core::message::Message>,
    ),
    Subagents(
        codeloom_core::session::SessionId,
        Vec<codeloom_core::session::SubagentDescriptor>,
    ),
    Error(String),
}

pub struct ProviderSettingsCommand {
    pub settings: ProviderSettings,
    pub reply: oneshot::Sender<Result<(), String>>,
}

/// Eventos de teclado que se procesan como máximo antes de pintar.
const MAX_BURST: usize = 512;

/// Restaura el terminal al salir, pase lo que pase (retorno normal, error o
/// panic): sin esto, un fallo en modo raw dejaba la terminal inutilizable.
struct RawGuard;

impl RawGuard {
    /// Entra en modo raw y en la pantalla alternativa. La captura del ratón solo
    /// se activa si `mouse` es `true`; sin ella, la selección nativa del terminal sigue disponible.
    fn enter(mouse: bool) -> std::io::Result<Self> {
        enable_raw_mode()?;
        let guard = Self;
        execute!(
            std::io::stdout(),
            EnterAlternateScreen,
            EnableBracketedPaste,
        )?;
        if mouse {
            execute!(std::io::stdout(), EnableMouseCapture)?;
        }
        Ok(guard)
    }
}

impl Drop for RawGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            std::io::stdout(),
            EndSynchronizedUpdate,
            crossterm::cursor::Show,
            DisableBracketedPaste,
            DisableMouseCapture,
            LeaveAlternateScreen
        );
    }
}

/// Teclas con el foco en el transcript (como Crush).
fn transcript_key(app: &mut App, code: KeyCode) {
    let len = app.transcript.len();
    if let Some(mut s) = app.selection {
        match code {
            KeyCode::Up | KeyCode::Char('k') => s.extend(-1, len),
            KeyCode::Down | KeyCode::Char('j') => s.extend(1, len),
            KeyCode::Char('y') | KeyCode::Char('c') => {
                app.status = format!("copiado ({} líneas)", s.range().count());
                app.clipboard = Some(s.text(&app.transcript));
                app.selection = None;
                return;
            }
            KeyCode::Char('v') => {
                app.selection = None;
                return;
            }
            _ => return,
        }
        app.selection = Some(s);
        return;
    }
    if code == KeyCode::Char('v') && len > 0 {
        // La selección de líneas y la de texto son excluyentes.
        app.text_sel = None;
        app.selection = Some(batten::state::Selection::at(len - 1));
        return;
    }
    let vp = &mut app.viewport;
    match code {
        KeyCode::Up | KeyCode::Char('k') => vp.scroll_up(1),
        KeyCode::Down | KeyCode::Char('j') => vp.scroll_down(1),
        KeyCode::Char('u') => vp.half_page_up(),
        KeyCode::Char('d') => vp.half_page_down(),
        KeyCode::PageUp | KeyCode::Char('b') => vp.page_up(),
        KeyCode::PageDown | KeyCode::Char('f') | KeyCode::Char(' ') => vp.page_down(),
        KeyCode::Home | KeyCode::Char('g') => vp.to_top(),
        KeyCode::End | KeyCode::Char('G') => vp.follow_bottom(),
        _ => {}
    }
}

/// Traduce una tecla a una accion sobre la app. Devuelve el prompt a enviar
/// cuando el usuario pulsa Enter con texto.
/// Traduce una tecla de crossterm a la del diálogo de preguntas.
fn question_key_of(code: KeyCode) -> Option<batten::state::QuestionKey> {
    use batten::state::QuestionKey;
    Some(match code {
        KeyCode::Up => QuestionKey::Up,
        KeyCode::Down => QuestionKey::Down,
        KeyCode::Left => QuestionKey::Left,
        KeyCode::Right => QuestionKey::Right,
        KeyCode::Enter => QuestionKey::Enter,
        KeyCode::Esc => QuestionKey::Esc,
        KeyCode::Backspace => QuestionKey::Backspace,
        KeyCode::Char(c) => QuestionKey::Char(c),
        _ => return None,
    })
}

pub fn handle_key(app: &mut App, code: KeyCode, mods: KeyModifiers) -> Option<String> {
    if app.slash_query().is_none() {
        app.slash_feedback = None;
    }
    if app
        .slash_dismissed
        .as_deref()
        .is_some_and(|text| text != app.composer.text())
    {
        app.slash_dismissed = None;
    }
    // Ctrl+C con una selección del composer la copia y no sale. Sin selección,
    // conserva su salida de siempre.
    if code == KeyCode::Char('c')
        && mods.contains(KeyModifiers::CONTROL)
        && app.composer.selection().is_some()
    {
        app.copy_composer_selection();
        return None;
    }
    // Igual con una selección de texto del transcript: la copia y no sale.
    if code == KeyCode::Char('c')
        && mods.contains(KeyModifiers::CONTROL)
        && app.text_sel.is_some_and(|sel| !sel.is_empty())
    {
        app.copy_transcript_selection();
        return None;
    }
    if matches!(code, KeyCode::Char('c' | 'q')) && mods.contains(KeyModifiers::CONTROL) {
        app.should_quit = true;
        return None;
    }
    if app.settings_open {
        handle_settings_key(app, code, mods);
        return None;
    }
    if app.help_open {
        if matches!(code, KeyCode::F(1) | KeyCode::Esc) {
            app.help_open = false;
        }
        return None;
    }
    if app.tools_open {
        match code {
            KeyCode::Esc => {
                app.tools_open = false;
                app.last_escape = None;
            }
            KeyCode::Char('t') if mods.contains(KeyModifiers::CONTROL) => app.tools_open = false,
            KeyCode::Up | KeyCode::Down => {
                app.tool_selected = if code == KeyCode::Up {
                    app.tool_selected.saturating_sub(1)
                } else {
                    (app.tool_selected + 1).min(app.tools.len().saturating_sub(1))
                };
                app.tool_scroll = (0, 0);
            }
            KeyCode::PageUp => app.tool_scroll.0 = app.tool_scroll.0.saturating_sub(10),
            KeyCode::PageDown => app.tool_scroll.0 = app.tool_scroll.0.saturating_add(10),
            KeyCode::Left => app.tool_scroll.1 = app.tool_scroll.1.saturating_sub(5),
            KeyCode::Right => app.tool_scroll.1 = app.tool_scroll.1.saturating_add(5),
            KeyCode::Home => app.tool_scroll = (0, 0),
            KeyCode::End => app.tool_scroll.0 = u16::MAX,
            KeyCode::Char('y') => {
                app.clipboard = app
                    .tools
                    .get(app.tool_selected)
                    .and_then(|tool| tool.output.clone());
                if app.clipboard.is_some() {
                    app.status = "tool output copied".into();
                }
            }
            _ => {}
        }
        return None;
    }
    if app.queue_open && app.queue_editing {
        handle_queue_editor_key(app, code, mods);
        return None;
    }
    if app.slash_query().is_some() {
        app.slash_feedback = None;
        let matches = app.slash_matches();
        app.slash_selected = app.slash_selected.min(matches.len().saturating_sub(1));
        match code {
            KeyCode::Esc => {
                app.slash_dismissed = Some(app.composer.text().to_string());
                app.last_escape = None;
                return None;
            }
            KeyCode::Up | KeyCode::Down => {
                app.slash_selected = if code == KeyCode::Up {
                    app.slash_selected.saturating_sub(1)
                } else {
                    (app.slash_selected + 1).min(matches.len().saturating_sub(1))
                };
                return None;
            }
            KeyCode::Tab => {
                if let Some((_, name, _)) = matches.get(app.slash_selected) {
                    app.composer.set_text(name);
                    app.slash_selected = 0;
                }
                return None;
            }
            KeyCode::Enter => {
                let Some((command, _, _)) = matches.get(app.slash_selected) else {
                    app.slash_feedback =
                        Some("Unknown command · /help for commands · Esc to send as text".into());
                    return None;
                };
                return execute_slash_command(app, *command);
            }
            _ => app.slash_selected = 0,
        }
    }
    if app.session_picker_open {
        if code == KeyCode::Esc {
            if app.session_searching {
                app.session_searching = false;
                app.session_query.clear();
                app.session_selected = 0;
            } else if app.session_picker_return_to_chat {
                app.session_picker_open = false;
                app.session_picker_return_to_chat = false;
                app.last_escape = None;
            } else {
                app.should_quit = true;
            }
            return None;
        }
        if code == KeyCode::Char('f') && mods.contains(KeyModifiers::CONTROL) {
            app.session_searching = !app.session_searching;
            app.session_selected = 0;
            return None;
        }
        if code == KeyCode::F(3) && !app.turn_in_flight {
            app.session_picker_open = false;
            app.session_picker_return_to_chat = true;
            app.session_list_refresh_requested = true;
            app.session_loading = true;
            app.session_query.clear();
            app.session_searching = false;
            return None;
        }
        if app.session_searching {
            match code {
                KeyCode::Char(c) if !mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                    app.session_query.push(c);
                    app.session_selected = 0;
                }
                KeyCode::Backspace => {
                    app.session_query.pop();
                    app.session_selected = 0;
                }
                _ => {}
            }
        }
        let matches = app.matching_sessions().cloned().collect::<Vec<_>>();
        let choices = matches.len() + 1;
        match code {
            KeyCode::Up | KeyCode::Char('k') if !app.session_searching => {
                app.session_selected = app.session_selected.saturating_sub(1)
            }
            KeyCode::Down | KeyCode::Char('j') if !app.session_searching => {
                app.session_selected = (app.session_selected + 1).min(choices - 1)
            }
            KeyCode::Home => app.session_selected = 0,
            KeyCode::End => app.session_selected = choices - 1,
            KeyCode::Char('n' | 'N') if !app.session_searching => {
                app.session_choice = Some(SessionChoice::New);
                app.session_picker_open = false;
                app.session_picker_return_to_chat = false;
                app.session_loading = true;
            }
            KeyCode::Enter => {
                app.session_choice = Some(if app.session_selected == matches.len() {
                    SessionChoice::New
                } else {
                    SessionChoice::Resume(matches[app.session_selected].id)
                });
                app.session_picker_open = false;
                app.session_picker_return_to_chat = false;
                app.session_loading = true;
            }
            _ => {}
        }
        app.session_selected = app.session_selected.min(choices - 1);
        return None;
    }
    // Con un diálogo de preguntas abierto (y sin permiso encima), Esc cancela la
    // pregunta en vez de contar para la salida por doble Esc.
    let question_has_keys = app.question.is_some() && app.permission.is_none();
    if code == KeyCode::Esc && !question_has_keys {
        // Con una selección de texto del transcript, Esc la limpia sin contar para
        // la doble Esc, igual que la del composer.
        if app.text_sel.is_some() && app.permission.is_none() {
            app.text_sel = None;
            app.last_escape = None;
            return None;
        }
        // Con una selección del composer, Esc solo la limpia y no cuenta para la
        // doble Esc que sale de la aplicación.
        if app.composer.selection().is_some() && app.permission.is_none() {
            app.composer.clear_selection();
            app.last_escape = None;
            return None;
        }
        if app.queue_open {
            app.queue_open = false;
            return None;
        }
        // Con foco en un panel lateral, Esc solo devuelve el foco al composer.
        // No cuenta para la doble Esc que sale de la aplicación.
        if app.focus.is_side_pane() && app.permission.is_none() {
            app.focus = Focus::Composer;
            app.last_escape = None;
            return None;
        }
        let now = Instant::now();
        if app
            .last_escape
            .is_some_and(|last| now.saturating_duration_since(last) <= ESC_DOUBLE_PRESS)
        {
            app.should_quit = true;
            app.last_escape = None;
        } else {
            app.last_escape = Some(now);
            app.cancel_requested = app.turn_in_flight;
            if app.cancel_requested {
                app.wake_pending = false;
            }
        }
        return None;
    }
    app.last_escape = None;
    if app.session_loading {
        return None;
    }
    if let Some(p) = app.permission.as_mut() {
        if let Some(d) = crate::permission_flow::permission_key(&mut p.dialog, code) {
            app.resolve_permission(d);
        }
        return None;
    }
    if app.question.is_some() {
        // Las teclas no reconocidas se consumen: no llegan al composer ni a los atajos.
        if let Some(key) = question_key_of(code) {
            app.question_key(key);
        }
        return None;
    }
    if code == KeyCode::F(1) {
        app.help_open = true;
        return None;
    }
    if code == KeyCode::Char('t') && mods.contains(KeyModifiers::CONTROL) {
        app.tools_open = true;
        app.tool_selected = app.tools.len().saturating_sub(1);
        app.tool_scroll = (0, 0);
        return None;
    }
    if code == KeyCode::F(2) {
        app.open_settings();
        return None;
    }
    if code == KeyCode::F(3) && !app.turn_in_flight {
        app.session_picker_open = false;
        app.session_picker_return_to_chat = true;
        app.session_list_refresh_requested = true;
        app.session_loading = true;
        app.session_query.clear();
        app.session_searching = false;
        return None;
    }
    if code == KeyCode::Char('k') && mods.contains(KeyModifiers::CONTROL) {
        app.queue_open = !app.queue_open;
        app.queue_selected = app
            .queue_selected
            .min(app.pending_prompts.len().saturating_sub(1));
        return None;
    }
    if app.queue_open {
        let len = app.pending_prompts.len();
        match (code, mods.contains(KeyModifiers::CONTROL)) {
            (KeyCode::Char(' '), false) => app.queue_paused = !app.queue_paused,
            (KeyCode::Char('x'), true) if app.turn_in_flight => {
                app.queue_paused = true;
                app.request_cancel();
            }
            (KeyCode::Enter, false) if len > 0 => {
                app.queue_editor
                    .set_text(&app.pending_prompts[app.queue_selected]);
                app.queue_editing = true;
            }
            (KeyCode::Up, false) => app.queue_selected = app.queue_selected.saturating_sub(1),
            (KeyCode::Down, false) => {
                app.queue_selected = (app.queue_selected + 1).min(len.saturating_sub(1))
            }
            (KeyCode::Up, true) if app.queue_selected > 0 => {
                app.pending_prompts
                    .swap(app.queue_selected, app.queue_selected - 1);
                app.queue_selected -= 1;
            }
            (KeyCode::Down, true) if app.queue_selected + 1 < len => {
                app.pending_prompts
                    .swap(app.queue_selected, app.queue_selected + 1);
                app.queue_selected += 1;
            }
            (KeyCode::Delete, _) if len > 0 => {
                app.pending_prompts.remove(app.queue_selected);
                app.queue_selected = app.queue_selected.min(len.saturating_sub(2));
                app.queued = app.pending_prompts.len();
            }
            _ => {}
        }
        return None;
    }
    if matches!(code, KeyCode::Tab | KeyCode::BackTab) {
        app.toggle_focus();
        return None;
    }
    if code == KeyCode::Char('b') && mods.contains(KeyModifiers::CONTROL) {
        app.toggle_sidebar();
        return None;
    }
    if code == KeyCode::Char('o') && mods.contains(KeyModifiers::CONTROL) {
        let call_id = app.tools.back_mut().map(|tool| {
            tool.toggle_expanded();
            tool.call_id.clone()
        });
        if let Some(call_id) = call_id {
            app.sync_tool(&call_id);
        }
        return None;
    }
    // Una tecla imprimible con foco en un panel lateral devuelve el foco al
    // composer, y el carácter se escribe en él a continuación.
    if matches!(code, KeyCode::Char(_))
        && !mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        && app.focus.is_side_pane()
    {
        app.focus = Focus::Composer;
    }
    if app.focus == Focus::Transcript {
        transcript_key(app, code);
        return None;
    }
    // Escribir, borrar o ampliar la selección en el composer anula la del transcript.
    if matches!(
        code,
        KeyCode::Char(_) | KeyCode::Backspace | KeyCode::Delete | KeyCode::Enter
    ) || mods.contains(KeyModifiers::SHIFT)
    {
        app.text_sel = None;
    }
    match code {
        KeyCode::Enter => {
            let sent = app.submit();
            if sent.is_some() {
                app.viewport.follow_bottom();
            }
            sent
        }
        KeyCode::PageUp => {
            app.viewport.scroll_up(10);
            None
        }
        KeyCode::PageDown => {
            app.viewport.scroll_down(10);
            None
        }
        KeyCode::Char('j') if mods.contains(KeyModifiers::CONTROL) => {
            app.newline();
            None
        }
        KeyCode::Delete => {
            app.delete();
            None
        }
        // Shift+flechas y Shift+Inicio/Fin amplían la selección del composer.
        KeyCode::Left if mods.contains(KeyModifiers::SHIFT) => {
            app.composer.extend_left();
            None
        }
        KeyCode::Right if mods.contains(KeyModifiers::SHIFT) => {
            app.composer.extend_right();
            None
        }
        KeyCode::Up if mods.contains(KeyModifiers::SHIFT) => {
            app.composer.extend_up();
            None
        }
        KeyCode::Down if mods.contains(KeyModifiers::SHIFT) => {
            app.composer.extend_down();
            None
        }
        KeyCode::Home if mods.contains(KeyModifiers::SHIFT) => {
            app.composer.extend_home();
            None
        }
        KeyCode::End if mods.contains(KeyModifiers::SHIFT) => {
            app.composer.extend_end();
            None
        }
        KeyCode::Left => {
            app.move_left();
            None
        }
        KeyCode::Right => {
            app.move_right();
            None
        }
        KeyCode::Home => {
            app.move_home();
            None
        }
        KeyCode::End => {
            app.move_end();
            None
        }
        KeyCode::Up => {
            app.up();
            None
        }
        KeyCode::Down => {
            app.down();
            None
        }
        KeyCode::Backspace => {
            app.backspace();
            None
        }
        KeyCode::Char(c) => {
            app.push_char(c);
            None
        }
        _ => None,
    }
}

fn execute_slash_command(app: &mut App, command: crate::commands::Command) -> Option<String> {
    use crate::commands::Command;
    if matches!(command, Command::New | Command::Sessions) && app.turn_in_flight {
        app.slash_feedback = Some("Wait for the active turn before switching sessions".into());
        return None;
    }
    if command == Command::Settings && app.provider_settings.is_none() {
        app.slash_feedback = Some("Provider settings are unavailable".into());
        return None;
    }
    if command == Command::Cancel && !app.turn_in_flight {
        app.slash_feedback = Some("No active turn to cancel".into());
        return None;
    }
    app.composer.set_text("");
    app.slash_selected = 0;
    app.slash_dismissed = None;
    app.history.reset();
    match command {
        Command::Help => handle_key(app, KeyCode::F(1), KeyModifiers::NONE),
        Command::Sessions => handle_key(app, KeyCode::F(3), KeyModifiers::NONE),
        Command::Settings => handle_key(app, KeyCode::F(2), KeyModifiers::NONE),
        Command::Queue => handle_key(app, KeyCode::Char('k'), KeyModifiers::CONTROL),
        Command::Tools => handle_key(app, KeyCode::Char('t'), KeyModifiers::CONTROL),
        Command::Theme => {
            app.cycle_theme();
            confirm_theme(app);
            None
        }
        Command::Crush => {
            app.set_theme(batten::widgets::ThemePreset::Crush);
            confirm_theme(app);
            None
        }
        Command::Classic => {
            app.set_theme(batten::widgets::ThemePreset::Batten);
            confirm_theme(app);
            None
        }
        Command::Sidebar => handle_key(app, KeyCode::Char('b'), KeyModifiers::CONTROL),
        Command::Mouse => {
            app.toggle_mouse_capture();
            confirm_mouse(app);
            None
        }
        Command::Cancel => {
            app.queue_paused = true;
            app.request_cancel();
            None
        }
        Command::Pause | Command::Resume => {
            app.queue_paused = command == Command::Pause;
            app.slash_feedback = Some(if app.queue_paused {
                "Pending prompts paused".into()
            } else {
                "Pending prompts resumed".into()
            });
            None
        }
        Command::Quit => handle_key(app, KeyCode::Char('q'), KeyModifiers::CONTROL),
        Command::New => {
            app.session_choice = Some(SessionChoice::New);
            app.session_loading = true;
            None
        }
    }
}

/// Confirma el tema elegido en el composer. El look Crush oculta la cabecera con
/// el nombre del tema, así que se muestra aquí. Si el guardado falló, se conserva
/// ese aviso en lugar de la confirmación.
fn confirm_theme(app: &mut App) {
    let name = app.theme_preset.name();
    app.slash_feedback
        .get_or_insert_with(|| format!("Theme: {name}"));
}

/// Confirma la captura del ratón elegida en el composer. Si el guardado falló,
/// se conserva ese aviso en lugar de la confirmación.
fn confirm_mouse(app: &mut App) {
    let message = if app.mouse_capture {
        "Mouse capture on (wheel scrolls)"
    } else {
        "Mouse capture off — select text with the mouse"
    };
    app.slash_feedback
        .get_or_insert_with(|| message.to_string());
}

fn handle_queue_editor_key(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    match code {
        KeyCode::Esc => app.queue_editing = false,
        KeyCode::Enter => {
            if !app.queue_editor.text().trim().is_empty() {
                if let Some(prompt) = app.pending_prompts.get_mut(app.queue_selected) {
                    *prompt = app.queue_editor.text().to_string();
                }
                app.queue_editing = false;
            }
        }
        KeyCode::Char('j') if mods.contains(KeyModifiers::CONTROL) => app.queue_editor.newline(),
        KeyCode::Char('a') if mods.contains(KeyModifiers::CONTROL) => app.queue_editor.set_text(""),
        KeyCode::Backspace => app.queue_editor.backspace(),
        KeyCode::Delete => app.queue_editor.delete(),
        KeyCode::Left => app.queue_editor.move_left(),
        KeyCode::Right => app.queue_editor.move_right(),
        KeyCode::Up => app.queue_editor.move_up(),
        KeyCode::Down => app.queue_editor.move_down(),
        KeyCode::Home => app.queue_editor.move_home(),
        KeyCode::End => app.queue_editor.move_end(),
        KeyCode::Char(c) if !mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
            app.queue_editor.insert_char(c);
        }
        _ => {}
    }
}

fn handle_settings_key(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    if app.settings_saving {
        if code == KeyCode::Esc {
            app.settings_open = false;
        }
        return;
    }
    if app.settings_editing {
        match code {
            KeyCode::Esc => app.settings_editing = false,
            KeyCode::Enter => app.commit_settings_value(),
            KeyCode::Char('a') if mods.contains(KeyModifiers::CONTROL) => {
                app.settings_editor.set_text("")
            }
            KeyCode::Backspace => app.settings_editor.backspace(),
            KeyCode::Delete => app.settings_editor.delete(),
            KeyCode::Left => app.settings_editor.move_left(),
            KeyCode::Right => app.settings_editor.move_right(),
            KeyCode::Home => app.settings_editor.move_home(),
            KeyCode::End => app.settings_editor.move_end(),
            KeyCode::Char(c) if !mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                app.settings_editor.insert_char(c);
            }
            _ => {}
        }
        return;
    }
    match code {
        KeyCode::Esc => app.settings_open = false,
        KeyCode::Up => app.settings_field = app.settings_field.saturating_sub(1),
        KeyCode::Down => app.settings_field = (app.settings_field + 1).min(4),
        KeyCode::Left | KeyCode::Right if app.settings_field == 0 => toggle_provider(app),
        KeyCode::Enter if app.settings_field == 0 => {
            toggle_provider(app);
        }
        KeyCode::Enter if (1..=3).contains(&app.settings_field) => app.edit_settings_value(),
        KeyCode::Enter if app.settings_field == 4 => app.request_settings_save(),
        KeyCode::Char('s') if mods.contains(KeyModifiers::CONTROL) => app.request_settings_save(),
        _ => {}
    }
}

fn toggle_provider(app: &mut App) {
    if let Some(settings) = app.provider_settings.as_mut() {
        settings.kind = match settings.kind {
            codeloom_core::config::ProviderKind::OpenAiCompat => {
                codeloom_core::config::ProviderKind::Anthropic
            }
            codeloom_core::config::ProviderKind::Anthropic => {
                codeloom_core::config::ProviderKind::OpenAiCompat
            }
        };
    }
}

/// Aplica un evento de terminal. Devuelve el prompt a enviar, si lo hay.
pub fn handle_event(app: &mut App, ev: Event) -> Option<String> {
    // Teclado y pegado pueden cambiar el texto: cualquiera anula el arrastre en curso.
    if matches!(ev, Event::Key(_) | Event::Paste(_)) {
        app.mouse_drag = None;
    }
    match ev {
        Event::Key(k) if k.kind == KeyEventKind::Press => handle_key(app, k.code, k.modifiers),
        Event::Key(k)
            if k.kind == KeyEventKind::Repeat
                && !k
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                && app.permission.is_none()
                && !app.session_picker_open
                && !app.session_loading
                && !app.queue_open
                && !app.settings_open
                && !app.help_open
                && !app.tools_open
                && match k.code {
                    KeyCode::Char(_) => app.focus == Focus::Composer,
                    KeyCode::Left
                    | KeyCode::Right
                    | KeyCode::Up
                    | KeyCode::Down
                    | KeyCode::Home
                    | KeyCode::End
                    | KeyCode::PageUp
                    | KeyCode::PageDown
                    | KeyCode::Backspace
                    | KeyCode::Delete => true,
                    _ => false,
                } =>
        {
            handle_key(app, k.code, k.modifiers)
        }
        Event::Mouse(mouse)
            if !app.session_picker_open
                && !app.session_loading
                && !app.settings_open
                && !app.help_open
                && !app.tools_open
                && !app.queue_open
                && app.permission.is_none()
                && app.question.is_none() =>
        {
            match mouse.kind {
                MouseEventKind::ScrollUp => app.viewport.scroll_up(3),
                MouseEventKind::ScrollDown => app.viewport.scroll_down(3),
                MouseEventKind::Down(MouseButton::Left) => app.mouse_down(mouse.column, mouse.row),
                MouseEventKind::Drag(MouseButton::Left) => {
                    app.mouse_drag_to(mouse.column, mouse.row);
                }
                MouseEventKind::Up(MouseButton::Left) => app.mouse_up(),
                _ => {}
            }
            None
        }
        // Con un diálogo abierto el ratón no llega a la interfaz: un arrastre
        // pendiente se descarta para que no sobreviva al diálogo.
        Event::Mouse(_) => {
            app.mouse_drag = None;
            app.text_drag = false;
            None
        }
        Event::Paste(text)
            if !app.session_picker_open
                && !app.session_loading
                && !app.help_open
                && !app.tools_open
                && app.permission.is_none()
                && app.question.is_none() =>
        {
            if app.settings_open {
                if app.settings_editing && !app.settings_saving {
                    app.settings_editor
                        .insert_str(&text.replace(['\r', '\n'], ""));
                }
            } else if app.queue_open && app.queue_editing {
                app.queue_editor.insert_str(&text);
            } else if !app.queue_open {
                app.text_sel = None;
                app.paste(&text);
            }
            None
        }
        _ => None,
    }
}

/// Encola prompts y mantiene un solo turno enviado al worker. Un mensaje del
/// usuario rearma el presupuesto de turnos automáticos.
fn send(app: &mut App, tx: &UnboundedSender<String>, prompt: Option<String>) {
    if let Some(text) = prompt {
        app.rearm_auto_turns();
        app.pending_prompts.push_back(text);
        app.queued = app.pending_prompts.len();
    }
    dispatch_next(app, tx);
}

/// Pide un despertar del padre si la interfaz lo permite. Si el canal del
/// trabajador está cerrado, suelta la marca para no bloquear la cola de prompts.
fn dispatch_wake(app: &mut App, wake_tx: &UnboundedSender<()>) {
    if app.take_wake() && wake_tx.send(()).is_err() {
        app.wake_in_flight = false;
        app.status = "error: wake channel closed".into();
    }
}

fn dispatch_next(app: &mut App, tx: &UnboundedSender<String>) {
    // Un despertar en vuelo ocupa el turno: no se solapa un prompt con él.
    if app.turn_in_flight
        || app.wake_in_flight
        || app.queue_open
        || app.queue_paused
        || app.session_loading
        || app.session_picker_open
        || app.settings_open
        || app.settings_saving
    {
        return;
    }
    if let Some(prompt) = app.pending_prompts.pop_front() {
        match tx.send(prompt.clone()) {
            Ok(()) => app.turn_in_flight = true,
            Err(_) => {
                app.status = "error: agent channel closed".into();
                app.pending_prompts.push_front(prompt);
            }
        }
        app.queued = app.pending_prompts.len();
        app.queue_selected = app.queue_selected.min(app.queued.saturating_sub(1));
    }
}

fn cancel_active_turn(app: &mut App, active_turn: &ActiveTurn) {
    if !app.cancel_requested {
        return;
    }
    if let Ok(active) = active_turn.lock() {
        if let Some(token) = active.as_ref() {
            token.cancel();
            app.cancel_requested = false;
            app.status = "cancelling".into();
            while app.permission.is_some() {
                app.resolve_permission(codeloom_core::permission::Decision::Deny);
            }
            app.cancel_question();
        }
    }
}

/// Bucle principal por eventos: teclado (prioritario), permisos, bus del
/// agente, vencimiento de frame y animaciones. Solo pinta cuando hay cambios;
/// las ráfagas de teclado se agrupan en un frame; cada frame es atómico.
// Los canales tienen tipos distintos y los coordina este bucle único.
#[allow(clippy::too_many_arguments)]
pub async fn run(
    mut app: App,
    prompt_tx: UnboundedSender<String>,
    // Canal de despertares del padre ocioso: el trabajador ejecuta un turno sin mensaje de usuario.
    wake_tx: UnboundedSender<()>,
    settings_tx: UnboundedSender<ProviderSettingsCommand>,
    mut bus_rx: tokio::sync::broadcast::Receiver<codeloom_core::event::LiveEvent>,
    mut perm_rx: tokio::sync::mpsc::Receiver<(
        codeloom_core::permission::PermissionRequest,
        crate::permission_flow::PermissionReply,
    )>,
    mut ask_rx: tokio::sync::mpsc::Receiver<(
        Vec<codeloom_core::question::Question>,
        crate::permission_flow::QuestionReply,
    )>,
    active_turn: ActiveTurn,
    session_command_tx: UnboundedSender<SessionCommand>,
    mut session_update_rx: tokio::sync::mpsc::UnboundedReceiver<SessionUpdate>,
) -> std::io::Result<()> {
    let _guard = RawGuard::enter(app.mouse_capture)?;
    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend)?;
    let (input_tx, mut input_rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::task::spawn_blocking(move || loop {
        if input_tx.is_closed() {
            break;
        }
        match crossterm::event::poll(Duration::from_millis(50)) {
            Ok(true) => match crossterm::event::read() {
                Ok(event) => {
                    if input_tx.send(Ok(event)).is_err() {
                        break;
                    }
                }
                Err(error) => {
                    let _ = input_tx.send(Err(error));
                    break;
                }
            },
            Ok(false) => {}
            Err(error) => {
                let _ = input_tx.send(Err(error));
                break;
            }
        }
    });
    let mut frames = FrameScheduler::new(MIN_FRAME);
    let mut anim = tokio::time::interval(batten::widgets::FRAME_INTERVAL);
    anim.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut fx: Option<batten::widgets::fx::Effect> = None;
    let mut fx_clock = Instant::now();
    let (mut bus_open, mut perm_open, mut ask_open) = (true, true, true);
    let mut session_update_open = true;
    let mut pending_settings: Option<oneshot::Receiver<Result<(), String>>> = None;
    send_session_commands(&mut app, &session_command_tx);
    // Pintar antes de esperar eventos para que la TUI aparezca al arrancar.
    execute!(std::io::stdout(), BeginSynchronizedUpdate)?;
    terminal.draw(|f| render(f, &mut app))?;
    execute!(std::io::stdout(), EndSynchronizedUpdate)?;
    frames.drawn(Instant::now());

    while !app.should_quit {
        let animating = (fx.is_some() && app.permission.is_some())
            || app.matrix_rain.animating()
            || app.thinking
            || app.tools.iter().any(|tool| tool.is_spinning());
        let deadline = frames.deadline();
        tokio::select! {
            biased;
            ev = input_rx.recv() => match ev {
                Some(Ok(ev)) => {
                    let prompt = handle_event(&mut app, ev);
                    send_provider_settings(&mut app, &settings_tx, &mut pending_settings);
                    send(&mut app, &prompt_tx, prompt);
                    send_session_commands(&mut app, &session_command_tx);
                    cancel_active_turn(&mut app, &active_turn);
                    for _ in 0..MAX_BURST {
                        match input_rx.try_recv() {
                            Ok(Ok(ev)) => {
                                let prompt = handle_event(&mut app, ev);
                                send_provider_settings(&mut app, &settings_tx, &mut pending_settings);
                                send(&mut app, &prompt_tx, prompt);
                                send_session_commands(&mut app, &session_command_tx);
                                cancel_active_turn(&mut app, &active_turn);
                                if app.should_quit {
                                    break;
                                }
                            }
                            Ok(Err(error)) => return Err(error),
                            Err(_) => break,
                        }
                    }
                    dispatch_wake(&mut app, &wake_tx);
                    frames.request(Urgency::Input);
                }
                Some(Err(error)) => return Err(error),
                None => break,
            },
            req = perm_rx.recv(), if perm_open => match req {
                Some((req, reply)) => {
                    app.enqueue_permission(req, reply);
                    frames.request(Urgency::Input);
                }
                None => perm_open = false,
            },
            ask = ask_rx.recv(), if ask_open => match ask {
                Some((questions, reply)) => {
                    app.enqueue_questions(questions, reply);
                    frames.request(Urgency::Input);
                }
                None => ask_open = false,
            },
            update = session_update_rx.recv(), if session_update_open => {
                match update {
                    Some(SessionUpdate::List(sessions)) => app.update_sessions(sessions),
                    Some(SessionUpdate::Loaded(session, messages)) => {
                        app.load_session(session, messages);
                    }
                    Some(SessionUpdate::Subagents(session, subagents)) => {
                        app.load_subagents(session, subagents);
                    }
                    Some(SessionUpdate::Error(error)) => {
                        app.session_load_failed();
                        app.status = format!("error: {error}");
                    }
                    None => {
                        session_update_open = false;
                        app.session_load_failed();
                        app.status = "error: session worker closed".into();
                    }
                }
                dispatch_next(&mut app, &prompt_tx);
                dispatch_wake(&mut app, &wake_tx);
                frames.request(Urgency::Input);
            },
            saved = async { pending_settings.as_mut().unwrap().await }, if pending_settings.is_some() => {
                pending_settings = None;
                app.settings_saved(saved.unwrap_or_else(|_| Err("Settings update failed".into())));
                dispatch_next(&mut app, &prompt_tx);
                dispatch_wake(&mut app, &wake_tx);
                frames.request(Urgency::Input);
            },
            ev = bus_rx.recv(), if bus_open => {
                match ev {
                    Ok(ev) => app.apply(ev),
                    Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => bus_open = false,
                }
                while let Ok(ev) = bus_rx.try_recv() {
                    app.apply(ev);
                }
                dispatch_next(&mut app, &prompt_tx);
                dispatch_wake(&mut app, &wake_tx);
                frames.request(Urgency::Background);
            }
            _ = tokio::time::sleep_until(tokio::time::Instant::from_std(
                deadline.unwrap_or_else(Instant::now),
            )), if deadline.is_some() => {}
            _ = anim.tick(), if animating => {
                app.matrix_rain.advance();
                app.animation.advance();
                frames.request(Urgency::Background);
            },
        }

        cancel_active_turn(&mut app, &active_turn);
        if std::mem::take(&mut app.fx_pending) {
            fx = Some(batten::widgets::fx::dialog_open(&app.theme));
            fx_clock = Instant::now();
        }
        if let Some(text) = app.clipboard.take() {
            use std::io::Write;
            let mut out = std::io::stdout();
            let _ = out.write_all(crate::clipboard::osc52(&text).as_bytes());
            let _ = out.flush();
        }
        if std::mem::take(&mut app.mouse_capture_dirty) {
            if app.mouse_capture {
                execute!(std::io::stdout(), EnableMouseCapture)?;
            } else {
                execute!(std::io::stdout(), DisableMouseCapture)?;
            }
        }

        let now = Instant::now();
        if frames.due(now) {
            let elapsed = now.duration_since(fx_clock);
            fx_clock = now;
            execute!(std::io::stdout(), BeginSynchronizedUpdate)?;
            terminal.draw(|f| {
                render(f, &mut app);
                if let (Some(e), Some(permission)) = (fx.as_mut(), app.permission.as_ref()) {
                    let area = batten::widgets::permission::area(f.area(), &permission.dialog);
                    e.process(elapsed.into(), f.buffer_mut(), area);
                }
            })?;
            execute!(std::io::stdout(), EndSynchronizedUpdate)?;
            if fx.as_ref().is_some_and(|e| e.done()) {
                fx = None;
            }
            frames.drawn(now);
        }
    }
    if let Ok(active) = active_turn.lock() {
        if let Some(token) = active.as_ref() {
            token.cancel();
        }
    }
    app.cancel_question();
    Ok(())
}

fn send_provider_settings(
    app: &mut App,
    tx: &UnboundedSender<ProviderSettingsCommand>,
    pending: &mut Option<oneshot::Receiver<Result<(), String>>>,
) {
    if !std::mem::take(&mut app.settings_save_requested) || pending.is_some() {
        return;
    }
    let Some(settings) = app.provider_settings.clone() else {
        return;
    };
    let (reply, receiver) = oneshot::channel();
    match tx.send(ProviderSettingsCommand { settings, reply }) {
        Ok(()) => {
            app.settings_saving = true;
            *pending = Some(receiver);
        }
        Err(_) => app.settings_feedback = Some("Settings worker is unavailable".into()),
    }
}

fn send_session_commands(app: &mut App, tx: &UnboundedSender<SessionCommand>) {
    if std::mem::take(&mut app.session_list_refresh_requested) {
        let _ = tx.send(SessionCommand::Refresh);
    }
    if let Some(choice) = app.session_choice.take() {
        let _ = tx.send(SessionCommand::Select(choice));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_dialog(
        app: &mut App,
    ) -> tokio::sync::oneshot::Receiver<codeloom_core::permission::Decision> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        app.enqueue_permission(
            codeloom_core::permission::PermissionRequest {
                tool: "bash".into(),
                action: "run".into(),
                detail: r#"{"command":"ls"}"#.into(),
            },
            tx,
        );
        rx
    }

    #[test]
    fn dialog_captures_keys_and_answers() {
        use codeloom_core::permission::Decision;
        let mut app = App::new();
        let mut rx = open_dialog(&mut app);
        assert!(app.fx_pending);
        handle_key(&mut app, KeyCode::Char('a'), KeyModifiers::NONE);
        assert_eq!(app.composer.text(), "");
        assert!(app.permission.is_none());
        assert_eq!(rx.try_recv().unwrap(), Decision::Allow);
    }

    #[test]
    fn double_esc_and_ctrl_c_quit_even_with_permission_open() {
        let mut app = App::new();
        let _rx = open_dialog(&mut app);
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(!app.should_quit);
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(app.should_quit);

        let mut app = App::new();
        let _rx = open_dialog(&mut app);
        handle_key(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(app.should_quit);
    }

    #[test]
    fn session_picker_selects_existing_or_new_session() {
        let mut app = App::new();
        app.set_sessions(vec![
            codeloom_core::session::SessionInfo {
                id: codeloom_core::session::SessionId(1),
                preview: "primera".into(),
                message_count: 2,
            },
            codeloom_core::session::SessionInfo {
                id: codeloom_core::session::SessionId(2),
                preview: "segunda".into(),
                message_count: 4,
            },
        ]);
        handle_key(&mut app, KeyCode::End, KeyModifiers::NONE);
        assert_eq!(app.session_selected, 2);
        handle_key(&mut app, KeyCode::Char('k'), KeyModifiers::NONE);
        assert_eq!(app.session_selected, 1);
        handle_key(&mut app, KeyCode::Home, KeyModifiers::NONE);
        assert_eq!(app.session_selected, 0);
        handle_key(&mut app, KeyCode::Down, KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(
            app.session_choice,
            Some(SessionChoice::Resume(codeloom_core::session::SessionId(2)))
        );
        assert!(app.session_loading);

        let mut app = App::new();
        app.set_sessions(vec![codeloom_core::session::SessionInfo {
            id: codeloom_core::session::SessionId(1),
            preview: "primera".into(),
            message_count: 2,
        }]);
        handle_key(&mut app, KeyCode::Char('n'), KeyModifiers::NONE);
        assert_eq!(app.session_choice, Some(SessionChoice::New));

        let mut app = App::new();
        app.set_sessions(vec![
            codeloom_core::session::SessionInfo {
                id: codeloom_core::session::SessionId(1),
                preview: "primera".into(),
                message_count: 2,
            },
            codeloom_core::session::SessionInfo {
                id: codeloom_core::session::SessionId(2),
                preview: "segunda".into(),
                message_count: 4,
            },
        ]);
        handle_key(&mut app, KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(app.session_selected, 0);
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(
            app.session_choice,
            Some(SessionChoice::Resume(codeloom_core::session::SessionId(1)))
        );
        assert!(app.session_loading);
    }

    #[test]
    fn session_picker_filters_case_insensitively_and_resumes_match() {
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
        handle_key(&mut app, KeyCode::Char('f'), KeyModifiers::CONTROL);
        for c in "PROVIDER".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::NONE);
        }
        assert_eq!(app.matching_sessions().count(), 1);
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(
            app.session_choice,
            Some(SessionChoice::Resume(codeloom_core::session::SessionId(2)))
        );
    }

    #[test]
    fn f3_requests_session_picker_only_when_idle() {
        let mut app = App::new();
        handle_key(&mut app, KeyCode::F(3), KeyModifiers::NONE);
        assert!(app.session_loading);
        assert!(app.session_list_refresh_requested);
        assert!(app.session_picker_return_to_chat);

        let mut app = App::new();
        app.turn_in_flight = true;
        handle_key(&mut app, KeyCode::F(3), KeyModifiers::NONE);
        assert!(!app.session_loading);
        assert!(!app.session_list_refresh_requested);
    }

    #[test]
    fn escape_clears_session_search_before_quitting_picker() {
        let mut app = App::new();
        app.set_sessions(vec![codeloom_core::session::SessionInfo {
            id: codeloom_core::session::SessionId(1),
            preview: "Build frontend".into(),
            message_count: 2,
        }]);
        handle_key(&mut app, KeyCode::Char('f'), KeyModifiers::CONTROL);
        handle_key(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(app.session_picker_open);
        assert!(!app.session_searching);
        assert!(app.session_query.is_empty());
        assert!(!app.should_quit);
    }

    #[test]
    fn queued_requests_are_answered_in_order() {
        use codeloom_core::permission::Decision;
        let mut app = App::new();
        let mut first = open_dialog(&mut app);
        let mut second = open_dialog(&mut app);
        handle_key(&mut app, KeyCode::Char('d'), KeyModifiers::NONE);
        assert!(app.permission.is_some());
        handle_key(&mut app, KeyCode::Char('s'), KeyModifiers::NONE);
        assert_eq!(first.try_recv().unwrap(), Decision::Deny);
        assert_eq!(second.try_recv().unwrap(), Decision::AllowSession);
    }

    #[test]
    fn pasted_newlines_never_submit() {
        let mut app = App::new();
        let sent = handle_event(&mut app, Event::Paste("uno\ndos\n".into()));
        assert_eq!(sent, None);
        assert_eq!(app.composer.text(), "uno\ndos\n");
    }

    #[test]
    fn mouse_wheel_scrolls_transcript_but_not_overlays() {
        use crossterm::event::{MouseEvent, MouseEventKind};
        let mut app = App::new();
        app.viewport.set_geometry(20, 5);
        handle_event(
            &mut app,
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollUp,
                column: 0,
                row: 0,
                modifiers: KeyModifiers::NONE,
            }),
        );
        assert!(!app.viewport.is_following());

        app.queue_open = true;
        app.viewport.follow_bottom();
        handle_event(
            &mut app,
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollUp,
                column: 0,
                row: 0,
                modifiers: KeyModifiers::NONE,
            }),
        );
        assert!(app.viewport.is_following());
    }

    #[test]
    fn a_burst_of_keys_is_applied_in_order() {
        use crossterm::event::KeyEvent;
        let mut app = App::new();
        for c in "x".repeat(2000).chars() {
            handle_event(
                &mut app,
                Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)),
            );
        }
        assert_eq!(app.composer.text().len(), 2000);
    }

    /// Pinta la aplicación una vez en un terminal de prueba para que registre
    /// sus ventanas (`pane_rects`) y el área del composer.
    fn paint(app: &mut App) {
        let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
        term.draw(|f| render(f, app)).unwrap();
    }

    fn mouse_event(app: &mut App, kind: crossterm::event::MouseEventKind, x: u16, y: u16) {
        use crossterm::event::MouseEvent;
        handle_event(
            app,
            Event::Mouse(MouseEvent {
                kind,
                column: x,
                row: y,
                modifiers: KeyModifiers::NONE,
            }),
        );
    }

    /// Celda (x, y) de la primera línea del composer Classic, en la columna `col`
    /// del texto (el prefijo ocupa dos celdas tras el borde).
    fn composer_cell(app: &App, col: u16) -> (u16, u16) {
        let area = app
            .composer_area
            .expect("el composer se registra al pintar");
        (area.x + 1 + 2 + col, area.y + 1)
    }

    #[test]
    fn click_in_the_composer_moves_the_cursor_and_focuses_it() {
        use crossterm::event::{MouseButton, MouseEventKind};
        let mut app = App::new();
        app.composer.set_text("hello world");
        app.focus = Focus::Transcript;
        paint(&mut app);
        let (x, y) = composer_cell(&app, 4);
        mouse_event(&mut app, MouseEventKind::Down(MouseButton::Left), x, y);
        assert_eq!(app.focus, Focus::Composer);
        assert_eq!(app.composer.cursor(), 4);
        assert_eq!(app.composer.selection(), None);
    }

    #[test]
    fn click_in_another_pane_changes_focus_and_does_not_select() {
        use crossterm::event::{MouseButton, MouseEventKind};
        let mut app = App::new();
        app.composer.set_text("hello world");
        app.composer.set_cursor(3);
        paint(&mut app);
        let transcript = app
            .pane_rects
            .iter()
            .find(|(pane, _)| *pane == Focus::Transcript)
            .map(|(_, area)| *area)
            .expect("el transcript se registra");
        mouse_event(
            &mut app,
            MouseEventKind::Down(MouseButton::Left),
            transcript.x + 2,
            transcript.y + 2,
        );
        mouse_event(
            &mut app,
            MouseEventKind::Drag(MouseButton::Left),
            transcript.x + 4,
            transcript.y + 2,
        );
        mouse_event(
            &mut app,
            MouseEventKind::Up(MouseButton::Left),
            transcript.x + 4,
            transcript.y + 2,
        );
        assert_eq!(app.focus, Focus::Transcript);
        assert_eq!(app.composer.cursor(), 3);
        assert_eq!(app.composer.selection(), None);
        assert!(app.clipboard.is_none());
    }

    #[test]
    fn drag_in_the_composer_selects_text() {
        use crossterm::event::{MouseButton, MouseEventKind};
        let mut app = App::new();
        app.composer.set_text("hello world");
        paint(&mut app);
        let (x0, y) = composer_cell(&app, 0);
        let (x5, _) = composer_cell(&app, 5);
        mouse_event(&mut app, MouseEventKind::Down(MouseButton::Left), x0, y);
        mouse_event(&mut app, MouseEventKind::Drag(MouseButton::Left), x5, y);
        assert_eq!(app.composer.selection(), Some((0, 5)));
        assert_eq!(app.composer.selected_text(), Some("hello"));
    }

    #[test]
    fn drag_outside_the_composer_is_clamped_to_it() {
        use crossterm::event::{MouseButton, MouseEventKind};
        let mut app = App::new();
        app.composer.set_text("hello world");
        paint(&mut app);
        let (x6, y) = composer_cell(&app, 6);
        mouse_event(&mut app, MouseEventKind::Down(MouseButton::Left), x6, y);
        // Hacia arriba y a la izquierda: se acota al inicio del texto.
        mouse_event(&mut app, MouseEventKind::Drag(MouseButton::Left), 0, 0);
        assert_eq!(app.composer.selected_text(), Some("hello "));
        // Hacia abajo y a la derecha, muy lejos: se acota al final del texto.
        mouse_event(
            &mut app,
            MouseEventKind::Drag(MouseButton::Left),
            u16::MAX,
            u16::MAX,
        );
        assert_eq!(app.composer.selected_text(), Some("world"));
    }

    #[test]
    fn release_copies_the_selection_to_the_clipboard() {
        use crossterm::event::{MouseButton, MouseEventKind};
        let mut app = App::new();
        app.composer.set_text("hello world");
        paint(&mut app);
        let (x0, y) = composer_cell(&app, 0);
        let (x5, _) = composer_cell(&app, 5);
        mouse_event(&mut app, MouseEventKind::Down(MouseButton::Left), x0, y);
        mouse_event(&mut app, MouseEventKind::Drag(MouseButton::Left), x5, y);
        assert!(app.clipboard.is_none(), "copiar solo al soltar");
        mouse_event(&mut app, MouseEventKind::Up(MouseButton::Left), x5, y);
        assert_eq!(app.clipboard.as_deref(), Some("hello"));
        assert_eq!(app.slash_feedback.as_deref(), Some("Copied 5 chars"));
    }

    #[test]
    fn click_without_drag_copies_nothing() {
        use crossterm::event::{MouseButton, MouseEventKind};
        let mut app = App::new();
        app.composer.set_text("hello world");
        paint(&mut app);
        let (x, y) = composer_cell(&app, 3);
        mouse_event(&mut app, MouseEventKind::Down(MouseButton::Left), x, y);
        mouse_event(&mut app, MouseEventKind::Up(MouseButton::Left), x, y);
        assert!(app.clipboard.is_none());
        assert_eq!(app.slash_feedback, None);
    }

    /// Pinta a `w`×`h` y devuelve el búfer resultante, para localizar texto en pantalla.
    fn paint_sized(app: &mut App, w: u16, h: u16) -> ratatui::buffer::Buffer {
        let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
        term.draw(|f| render(f, app)).unwrap();
        term.backend().buffer().clone()
    }

    /// Celda (x, y) donde aparece por primera vez `needle` en la pantalla pintada.
    fn screen_pos_of(buf: &ratatui::buffer::Buffer, needle: &str) -> (u16, u16) {
        let area = buf.area;
        for y in area.y..area.bottom() {
            let row: String = (area.x..area.right())
                .map(|x| buf[(x, y)].symbol())
                .collect();
            if let Some(byte) = row.find(needle) {
                return (area.x + row[..byte].chars().count() as u16, y);
            }
        }
        panic!("«{needle}» no aparece en pantalla");
    }

    /// Aplicación con un mensaje de usuario por cada texto.
    fn transcript_app(texts: &[&str]) -> App {
        let mut app = App::new();
        for text in texts {
            app.push_user(text);
        }
        app
    }

    /// Arrastra con el botón izquierdo desde `from` hasta `to` y suelta.
    fn drag_select(app: &mut App, from: (u16, u16), to: (u16, u16)) {
        use crossterm::event::{MouseButton, MouseEventKind};
        mouse_event(app, MouseEventKind::Down(MouseButton::Left), from.0, from.1);
        mouse_event(app, MouseEventKind::Drag(MouseButton::Left), to.0, to.1);
        mouse_event(app, MouseEventKind::Up(MouseButton::Left), to.0, to.1);
    }

    #[test]
    fn click_in_transcript_focuses_and_starts_selection() {
        use crossterm::event::{MouseButton, MouseEventKind};
        let mut app = transcript_app(&["hola mundo"]);
        app.focus = Focus::Composer;
        let buf = paint_sized(&mut app, 100, 30);
        let (x, y) = screen_pos_of(&buf, "hola");
        mouse_event(&mut app, MouseEventKind::Down(MouseButton::Left), x, y);
        assert_eq!(app.focus, Focus::Transcript);
        let sel = app.text_sel.expect("el clic empieza una selección");
        assert!(sel.is_empty());
        assert!(app.text_drag);
    }

    #[test]
    fn drag_selects_text_and_mouse_up_copies_with_notice() {
        use crossterm::event::{MouseButton, MouseEventKind};
        let mut app = transcript_app(&["hola mundo"]);
        let buf = paint_sized(&mut app, 100, 30);
        let (x, y) = screen_pos_of(&buf, "hola");
        mouse_event(&mut app, MouseEventKind::Down(MouseButton::Left), x, y);
        mouse_event(&mut app, MouseEventKind::Drag(MouseButton::Left), x + 3, y);
        assert!(app.clipboard.is_none(), "copiar solo al soltar");
        mouse_event(&mut app, MouseEventKind::Up(MouseButton::Left), x + 3, y);
        assert_eq!(app.clipboard.as_deref(), Some("hola"));
        assert_eq!(app.slash_feedback.as_deref(), Some("Copied 4 chars"));
        assert!(app.text_sel.is_some(), "la selección sigue resaltada");
    }

    #[test]
    fn dragging_backwards_selects_the_same_text() {
        let mut app = transcript_app(&["hola mundo"]);
        let buf = paint_sized(&mut app, 100, 30);
        let (hx, y) = screen_pos_of(&buf, "hola");
        let (mx, _) = screen_pos_of(&buf, "mundo");
        // Del final hacia el principio: el rango se normaliza.
        drag_select(&mut app, (mx + 4, y), (hx, y));
        assert_eq!(app.clipboard.as_deref(), Some("hola mundo"));
    }

    #[test]
    fn simple_click_clears_selection_and_copies_nothing() {
        let mut app = transcript_app(&["hola mundo"]);
        let buf = paint_sized(&mut app, 100, 30);
        let (x, y) = screen_pos_of(&buf, "hola");
        drag_select(&mut app, (x, y), (x + 3, y));
        assert!(app.text_sel.is_some());
        app.clipboard = None;
        // Un clic simple, sin arrastre, descarta la selección anterior.
        drag_select(&mut app, (x, y), (x, y));
        assert!(app.text_sel.is_none());
        assert!(app.clipboard.is_none());
    }

    #[test]
    fn drag_above_area_autoscrolls_and_clamps() {
        let texts: Vec<String> = (0..60).map(|i| format!("msg-{i:02}")).collect();
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        let mut app = transcript_app(&refs);
        paint_sized(&mut app, 100, 30);
        assert!(app.viewport.is_following());
        let area = app.transcript_area.expect("el transcript fija su área");
        let (x, y) = (area.x, area.y + 2);
        let top = app.viewport.top();
        mouse_event_down_left(&mut app, x, y);
        for _ in 0..3 {
            mouse_event_drag_left(&mut app, x, 0);
        }
        assert_eq!(app.viewport.top(), top - 3, "una fila por evento");
        assert!(!app.viewport.is_following());
        let (start, _) = app.text_sel.expect("hay selección").ordered();
        assert_eq!(start.row, app.viewport.top(), "la cabeza se acota al área");
        // Por debajo del área también desplaza, sin salirse del transcript.
        let before = app.viewport.top();
        mouse_event_drag_left(&mut app, x, u16::MAX);
        assert_eq!(app.viewport.top(), before + 1);
    }

    /// Botón izquierdo pulsado en (x, y), para los tests que arrastran por pasos.
    fn mouse_event_down_left(app: &mut App, x: u16, y: u16) {
        use crossterm::event::{MouseButton, MouseEventKind};
        mouse_event(app, MouseEventKind::Down(MouseButton::Left), x, y);
    }

    /// Arrastre del botón izquierdo hasta (x, y).
    fn mouse_event_drag_left(app: &mut App, x: u16, y: u16) {
        use crossterm::event::{MouseButton, MouseEventKind};
        mouse_event(app, MouseEventKind::Drag(MouseButton::Left), x, y);
    }

    #[test]
    fn esc_clears_text_selection_without_double_esc() {
        let mut app = transcript_app(&["hola mundo"]);
        let buf = paint_sized(&mut app, 100, 30);
        let (x, y) = screen_pos_of(&buf, "hola");
        drag_select(&mut app, (x, y), (x + 3, y));
        assert!(app.text_sel.is_some());
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(app.text_sel.is_none());
        assert_eq!(app.last_escape, None);
        assert!(!app.should_quit);
        // La Esc que limpió no cuenta: hacen falta dos más para salir.
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(!app.should_quit);
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(app.should_quit);
    }

    #[test]
    fn ctrl_c_with_text_selection_copies_and_does_not_quit() {
        let mut app = transcript_app(&["hola mundo"]);
        let buf = paint_sized(&mut app, 100, 30);
        let (x, y) = screen_pos_of(&buf, "hola");
        drag_select(&mut app, (x, y), (x + 3, y));
        app.clipboard = None;
        handle_key(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(!app.should_quit);
        assert_eq!(app.clipboard.as_deref(), Some("hola"));
    }

    #[test]
    fn width_change_clears_text_selection() {
        let mut app = transcript_app(&["hola mundo"]);
        let buf = paint_sized(&mut app, 100, 30);
        let (x, y) = screen_pos_of(&buf, "hola");
        drag_select(&mut app, (x, y), (x + 3, y));
        assert!(app.text_sel.is_some());
        paint_sized(&mut app, 60, 30);
        assert!(app.text_sel.is_none());
    }

    #[test]
    fn text_selection_excludes_line_selection() {
        let mut app = transcript_app(&["uno", "dos", "tres"]);
        app.focus = Focus::Transcript;
        let buf = paint_sized(&mut app, 100, 30);
        handle_key(&mut app, KeyCode::Char('v'), KeyModifiers::NONE);
        assert!(app.selection.is_some());
        let (x, y) = screen_pos_of(&buf, "dos");
        drag_select(&mut app, (x, y), (x + 1, y));
        assert!(
            app.selection.is_none(),
            "la selección de texto anula la de líneas"
        );
        assert!(app.text_sel.is_some());
        handle_key(&mut app, KeyCode::Char('v'), KeyModifiers::NONE);
        assert!(app.text_sel.is_none(), "la de líneas anula la de texto");
        // Y la del composer, también excluyente.
        app.composer.set_text("hola");
        app.composer.select_range(0, 2);
        drag_select(&mut app, (x, y), (x + 1, y));
        assert_eq!(app.composer.selection(), None);
    }

    #[test]
    fn dialog_open_ignores_transcript_mouse() {
        let mut app = transcript_app(&["hola mundo"]);
        app.focus = Focus::Composer;
        let buf = paint_sized(&mut app, 100, 30);
        let _reply = open_dialog(&mut app);
        let (x, y) = screen_pos_of(&buf, "hola");
        drag_select(&mut app, (x, y), (x + 3, y));
        assert_eq!(app.focus, Focus::Composer);
        assert!(app.text_sel.is_none());
        assert!(!app.text_drag);
        assert!(app.clipboard.is_none());
    }

    #[test]
    fn drag_on_empty_transcript_does_not_panic() {
        let mut app = App::new();
        paint_sized(&mut app, 100, 30);
        drag_select(&mut app, (10, 10), (20, 5));
        assert!(app.text_sel.is_none());
        // Un terminal de una sola fila de transcript tampoco debe romper.
        let mut app = transcript_app(&["uno", "dos", "tres"]);
        paint_sized(&mut app, 100, 6);
        drag_select(&mut app, (10, 3), (12, 0));
        mouse_event_drag_left(&mut app, 5, u16::MAX);
    }

    #[test]
    fn shift_arrows_extend_the_selection() {
        let mut app = App::new();
        app.composer.set_text("abc");
        handle_key(&mut app, KeyCode::Left, KeyModifiers::SHIFT);
        handle_key(&mut app, KeyCode::Left, KeyModifiers::SHIFT);
        assert_eq!(app.composer.selected_text(), Some("bc"));
        handle_key(&mut app, KeyCode::Home, KeyModifiers::SHIFT);
        assert_eq!(app.composer.selected_text(), Some("abc"));

        let mut app = App::new();
        app.composer.set_text("abc");
        app.composer.set_cursor(0);
        handle_key(&mut app, KeyCode::End, KeyModifiers::SHIFT);
        assert_eq!(app.composer.selected_text(), Some("abc"));
        handle_key(&mut app, KeyCode::Right, KeyModifiers::NONE);
        assert_eq!(
            app.composer.selection(),
            None,
            "sin Shift, el movimiento limpia"
        );
    }

    #[test]
    fn ctrl_c_with_selection_copies_and_does_not_quit() {
        let mut app = App::new();
        app.composer.set_text("abc");
        app.composer.select_range(0, 2);
        handle_key(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(!app.should_quit);
        assert_eq!(app.clipboard.as_deref(), Some("ab"));
    }

    #[test]
    fn ctrl_c_without_selection_keeps_its_behavior() {
        let mut app = App::new();
        app.composer.set_text("abc");
        handle_key(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(app.should_quit);
        assert!(app.clipboard.is_none());
    }

    #[test]
    fn esc_with_selection_clears_it_and_does_not_count_for_double_esc() {
        let mut app = App::new();
        app.composer.set_text("abc");
        app.composer.select_range(0, 2);
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(app.composer.selection(), None);
        assert_eq!(app.last_escape, None);
        assert!(!app.should_quit);
        // La primera Esc sin selección cuenta, pero no basta para salir.
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(!app.should_quit);
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(app.should_quit);
    }

    #[test]
    fn mouse_is_ignored_while_a_dialog_is_open() {
        use crossterm::event::{MouseButton, MouseEventKind};
        let mut app = App::new();
        app.composer.set_text("hello world");
        app.composer.set_cursor(2);
        app.focus = Focus::Transcript;
        paint(&mut app);
        let _reply = open_dialog(&mut app);
        let (x, y) = composer_cell(&app, 6);
        mouse_event(&mut app, MouseEventKind::Down(MouseButton::Left), x, y);
        mouse_event(&mut app, MouseEventKind::Drag(MouseButton::Left), x + 3, y);
        mouse_event(&mut app, MouseEventKind::Up(MouseButton::Left), x + 3, y);
        assert_eq!(app.focus, Focus::Transcript);
        assert_eq!(app.composer.cursor(), 2);
        assert_eq!(app.composer.selection(), None);
        assert!(app.clipboard.is_none());
    }

    #[test]
    fn typing_after_selecting_replaces_the_selected_text() {
        let mut app = App::new();
        app.composer.set_text("hello world");
        app.composer.select_range(0, 5);
        handle_key(&mut app, KeyCode::Char('X'), KeyModifiers::NONE);
        assert_eq!(app.composer.text(), "X world");
        assert_eq!(app.composer.selection(), None);
    }

    #[test]
    fn typing_during_a_drag_cancels_it() {
        use crossterm::event::{MouseButton, MouseEventKind};
        let mut app = App::new();
        app.composer.set_text("hello world");
        paint(&mut app);
        let (x0, y) = composer_cell(&app, 0);
        let (x5, _) = composer_cell(&app, 5);
        mouse_event(&mut app, MouseEventKind::Down(MouseButton::Left), x0, y);
        handle_event(
            &mut app,
            Event::Key(crossterm::event::KeyEvent::new(
                KeyCode::Char('x'),
                KeyModifiers::NONE,
            )),
        );
        mouse_event(&mut app, MouseEventKind::Drag(MouseButton::Left), x5, y);
        assert_eq!(app.composer.selection(), None);
    }

    #[test]
    fn ctrl_c_quits() {
        let mut app = App::new();
        assert_eq!(
            handle_key(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL),
            None
        );
        assert!(app.should_quit);
    }

    #[test]
    fn settings_save_waits_for_idle_and_freezes_values_until_reply() {
        let mut app = App::new();
        app.set_provider_settings(ProviderSettings {
            kind: codeloom_core::config::ProviderKind::OpenAiCompat,
            model: "old-model".into(),
            base_url: "https://example.test/v1".into(),
            api_key_env: "API_KEY".into(),
        });
        app.open_settings();
        app.provider_settings.as_mut().unwrap().model = "new-model".into();
        app.turn_in_flight = true;
        handle_key(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert!(!app.settings_save_requested);
        assert!(app
            .settings_feedback
            .as_ref()
            .unwrap()
            .contains("active turn"));

        app.turn_in_flight = false;
        handle_key(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut pending = None;
        send_provider_settings(&mut app, &tx, &mut pending);
        let command = rx.try_recv().unwrap();
        assert_eq!(command.settings.model, "new-model");
        handle_key(&mut app, KeyCode::Right, KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Down, KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        handle_event(&mut app, Event::Paste("other-model".into()));
        handle_key(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert!(!app.settings_editing);
        assert!(!app.settings_save_requested);
        assert_eq!(
            app.provider_settings.as_ref().unwrap().kind,
            command.settings.kind
        );
        assert_eq!(app.model, "old-model");

        command
            .reply
            .send(Err("Cannot write config".into()))
            .unwrap();
        app.settings_saved(pending.take().unwrap().try_recv().unwrap());
        assert_eq!(app.model, "old-model");
        assert!(!app.settings_saving);
        assert_eq!(
            app.settings_feedback.as_deref(),
            Some("Cannot write config")
        );

        handle_key(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
        send_provider_settings(&mut app, &tx, &mut pending);
        rx.try_recv().unwrap().reply.send(Ok(())).unwrap();
        app.settings_saved(pending.take().unwrap().try_recv().unwrap());
        assert_eq!(app.model, "new-model");
        assert_eq!(
            app.settings_feedback.as_deref(),
            Some("Settings saved and applied")
        );

        drop(rx);
        app.request_settings_save();
        send_provider_settings(&mut app, &tx, &mut pending);
        assert!(pending.is_none());
        assert!(!app.settings_saving);
        assert_eq!(
            app.settings_feedback.as_deref(),
            Some("Settings worker is unavailable")
        );
    }

    #[test]
    fn permission_interrupts_overlays_and_ignores_paste_and_abandoned_requests() {
        let mut app = App::new();
        app.help_open = true;
        handle_event(&mut app, Event::Paste("hidden input".into()));
        assert!(app.composer.is_empty());
        app.settings_open = true;
        app.settings_editing = true;
        let mut first = open_dialog(&mut app);
        let abandoned = open_dialog(&mut app);
        let mut last = open_dialog(&mut app);
        drop(abandoned);
        assert!(!app.help_open);
        assert!(!app.settings_open);
        assert!(!app.settings_editing);
        handle_event(&mut app, Event::Paste("hidden input".into()));
        assert!(app.composer.is_empty());
        handle_key(&mut app, KeyCode::Char('d'), KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Char('a'), KeyModifiers::NONE);
        assert_eq!(
            first.try_recv().unwrap(),
            codeloom_core::permission::Decision::Deny
        );
        assert_eq!(
            last.try_recv().unwrap(),
            codeloom_core::permission::Decision::Allow
        );
        assert!(app.permission.is_none());
    }

    #[test]
    fn provider_settings_edit_values_and_request_save_without_touching_prompt() {
        let mut app = App::new();
        app.set_provider_settings(ProviderSettings {
            kind: codeloom_core::config::ProviderKind::OpenAiCompat,
            model: "old-model".into(),
            base_url: "https://example.test/v1".into(),
            api_key_env: "API_KEY".into(),
        });
        handle_key(&mut app, KeyCode::F(2), KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Right, KeyModifiers::NONE);
        assert_eq!(
            app.provider_settings.as_ref().unwrap().kind,
            codeloom_core::config::ProviderKind::Anthropic
        );
        handle_key(&mut app, KeyCode::Down, KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::End, KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Char('2'), KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert_eq!(app.provider_settings.as_ref().unwrap().model, "old-model2");
        assert!(app.settings_save_requested);
        assert!(app.composer.is_empty());
    }

    #[test]
    fn esc_and_ctrl_q_quit() {
        let mut app = App::new();
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(!app.should_quit);
        assert!(!app.cancel_requested);
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(app.should_quit);

        let mut app = App::new();
        handle_key(&mut app, KeyCode::Char('q'), KeyModifiers::CONTROL);
        assert!(app.should_quit);
    }

    #[test]
    fn f1_opens_help_and_escape_closes_it_without_canceling() {
        let mut app = App::new();
        handle_key(&mut app, KeyCode::F(1), KeyModifiers::NONE);
        assert!(app.help_open);
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(!app.help_open);
        assert!(!app.cancel_requested);
        assert!(!app.should_quit);
    }

    #[test]
    fn ctrl_b_toggles_sidebar_without_editing_composer() {
        let mut app = App::new();
        handle_key(&mut app, KeyCode::Char('b'), KeyModifiers::CONTROL);
        assert!(app.sidebar_expanded);
        assert!(app.composer.is_empty());
        handle_key(&mut app, KeyCode::Char('b'), KeyModifiers::CONTROL);
        assert!(!app.sidebar_expanded);
    }

    #[test]
    fn ctrl_o_toggles_latest_tool_output() {
        let mut app = App::new();
        app.tools
            .push_back(batten::state::ToolItem::new("call-1", "view"));
        handle_key(&mut app, KeyCode::Char('o'), KeyModifiers::CONTROL);
        assert!(app.tools.back().unwrap().expanded);
        handle_key(&mut app, KeyCode::Char('o'), KeyModifiers::CONTROL);
        assert!(!app.tools.back().unwrap().expanded);
    }

    #[test]
    fn esc_cancellation_waits_for_worker_token() {
        let mut app = App::new();
        app.turn_in_flight = true;
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        let active_turn: ActiveTurn = Arc::new(Mutex::new(None));
        cancel_active_turn(&mut app, &active_turn);
        assert!(app.cancel_requested);

        let token = CancellationToken::new();
        *active_turn.lock().unwrap() = Some(token.clone());
        cancel_active_turn(&mut app, &active_turn);
        assert!(token.is_cancelled());
        assert!(!app.cancel_requested);
        assert_eq!(app.status, "cancelling");
    }

    #[test]
    fn slash_controls_pause_cancel_resume_and_sidebar_without_dispatching_commands() {
        let mut app = App::new();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        app.turn_in_flight = true;
        let token = CancellationToken::new();
        let active = Arc::new(Mutex::new(Some(token.clone())));
        send(&mut app, &tx, Some("pending work".into()));
        for text in ["/pause", "/pause"] {
            app.composer.set_text(text);
            let prompt = handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
            send(&mut app, &tx, prompt);
            assert!(app.queue_paused);
            assert!(
                !token.is_cancelled(),
                "pause does not cancel the active turn"
            );
            assert!(rx.try_recv().is_err());
        }
        app.composer.set_text("/cancel");
        let prompt = handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        send(&mut app, &tx, prompt);
        cancel_active_turn(&mut app, &active);
        assert!(token.is_cancelled());
        app.apply(codeloom_core::event::LiveEvent::TurnEnded);
        dispatch_next(&mut app, &tx);
        assert!(rx.try_recv().is_err());
        assert_eq!(app.queued, 1);
        app.composer.set_text("/resume");
        let prompt = handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        send(&mut app, &tx, prompt);
        assert!(!app.queue_paused);
        assert_eq!(rx.try_recv().unwrap(), "pending work");
        assert!(rx.try_recv().is_err());
        for expanded in [true, false] {
            app.composer.set_text("/sidebar");
            assert_eq!(
                handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE),
                None
            );
            assert_eq!(app.sidebar_expanded, expanded);
        }
        app.apply(codeloom_core::event::LiveEvent::TurnEnded);
        app.composer.set_text("/cancel");
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(
            app.slash_feedback.as_deref(),
            Some("No active turn to cancel")
        );
        assert_eq!(app.composer.text(), "/cancel");
        assert!(!app.cancel_requested);
        assert!(!app.queue_paused);
    }

    #[test]
    fn turn_completion_discards_cancellation_that_arrived_after_the_worker_finished() {
        for event in [
            codeloom_core::event::LiveEvent::TurnEnded,
            codeloom_core::event::LiveEvent::Error {
                message: "failed".into(),
            },
        ] {
            let mut app = App::new();
            app.turn_in_flight = true;
            app.composer.set_text("/cancel");
            handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
            assert!(app.cancel_requested);
            app.apply(event);
            let next = CancellationToken::new();
            let active = Arc::new(Mutex::new(Some(next.clone())));
            app.turn_in_flight = true;
            cancel_active_turn(&mut app, &active);
            assert!(!next.is_cancelled());
            assert!(!app.cancel_requested);
            assert!(app.queue_paused);
        }
    }

    #[test]
    fn slash_menu_filters_completes_and_executes_without_sending_a_prompt() {
        let mut app = App::new();
        handle_event(&mut app, Event::Paste("/s".into()));
        assert_eq!(app.slash_matches().len(), 3);
        handle_key(&mut app, KeyCode::Down, KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(app.composer.text(), "/settings");
        assert_eq!(app.focus, Focus::Composer);
        app.set_provider_settings(ProviderSettings {
            kind: codeloom_core::config::ProviderKind::OpenAiCompat,
            model: "test".into(),
            base_url: "https://example.test".into(),
            api_key_env: "KEY".into(),
        });
        assert_eq!(
            handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE),
            None
        );
        assert!(app.settings_open);
        assert!(app.composer.is_empty());
        assert!(app.pending_prompts.is_empty());
        assert_eq!(app.queued, 0);
        assert!(!app.history.is_browsing());
    }

    #[test]
    fn slash_commands_share_existing_actions_and_session_guards() {
        for text in ["/help", "/new", "/sessions", "/queue", "/tools", "/quit"] {
            let mut app = App::new();
            app.composer.set_text(text);
            assert_eq!(
                handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE),
                None
            );
            match text {
                "/help" => assert!(app.help_open),
                "/new" => {
                    assert_eq!(app.session_choice, Some(SessionChoice::New));
                    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
                    send_session_commands(&mut app, &tx);
                    assert!(matches!(
                        rx.try_recv().unwrap(),
                        SessionCommand::Select(SessionChoice::New)
                    ));
                }
                "/sessions" => assert!(app.session_list_refresh_requested),
                "/queue" => assert!(app.queue_open),
                "/tools" => assert!(app.tools_open),
                _ => assert!(app.should_quit),
            }
            assert!(app.composer.is_empty());
        }
        for text in ["/new", "/sessions"] {
            let mut app = App::new();
            app.turn_in_flight = true;
            app.composer.set_text(text);
            handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
            assert_eq!(app.composer.text(), text);
            assert!(!app.session_loading);
            assert!(app.session_choice.is_none());
            assert!(app
                .slash_feedback
                .as_deref()
                .unwrap()
                .contains("active turn"));
        }
    }

    #[test]
    fn slash_escape_preserves_literal_text_and_unknown_commands_do_not_dispatch() {
        let mut app = App::new();
        app.turn_in_flight = true;
        app.composer.set_text("/unknown");
        assert_eq!(
            handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE),
            None
        );
        assert_eq!(app.composer.text(), "/unknown");
        assert!(app
            .slash_feedback
            .as_deref()
            .unwrap()
            .contains("Unknown command"));
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(!app.cancel_requested);
        assert!(!app.should_quit);
        assert!(app.slash_query().is_none());
        assert_eq!(
            handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE),
            Some("/unknown".into())
        );
        handle_key(&mut app, KeyCode::Char('/'), KeyModifiers::NONE);
        assert_eq!(app.slash_matches().len(), crate::commands::COMMANDS.len());

        for text in [
            "explain /help",
            "/tmp/file.rs",
            "//literal",
            "/help me",
            "/help\nsecond line",
        ] {
            let mut app = App::new();
            app.composer.set_text(text);
            assert!(app.slash_query().is_none());
            assert_eq!(
                handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE),
                Some(text.into())
            );
        }
        app.composer.set_text("/tools");
        let _reply = open_dialog(&mut app);
        assert!(app.slash_query().is_none());
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert!(!app.tools_open);
        assert_eq!(app.composer.text(), "/tools");
    }

    #[test]
    fn tool_inspector_navigates_copies_and_preserves_the_draft() {
        let mut app = App::new();
        app.composer.set_text("unfinished draft");
        let mut first = batten::state::ToolItem::new("first", "view");
        first.set_output("complete output\nsecond line");
        app.tools.push_back(first);
        app.tools
            .push_back(batten::state::ToolItem::new("second", "bash"));
        handle_key(&mut app, KeyCode::Char('t'), KeyModifiers::CONTROL);
        assert!(app.tools_open);
        assert_eq!(app.tool_selected, 1);
        handle_key(&mut app, KeyCode::Char('y'), KeyModifiers::NONE);
        assert!(app.clipboard.is_none());
        handle_key(&mut app, KeyCode::Up, KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Char('y'), KeyModifiers::NONE);
        assert_eq!(
            app.clipboard.as_deref(),
            Some("complete output\nsecond line")
        );
        handle_key(&mut app, KeyCode::PageDown, KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Right, KeyModifiers::NONE);
        assert_eq!(app.tool_scroll, (10, 5));
        handle_key(&mut app, KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(app.tool_scroll, (0, 0));
        handle_event(&mut app, Event::Paste("hidden input".into()));
        assert_eq!(app.composer.text(), "unfinished draft");
        let _reply = open_dialog(&mut app);
        assert!(!app.tools_open);
        handle_key(&mut app, KeyCode::Char('d'), KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Char('t'), KeyModifiers::CONTROL);
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(!app.tools_open);
        assert!(!app.cancel_requested);
        assert!(!app.should_quit);

        let mut empty = App::new();
        handle_key(&mut empty, KeyCode::Char('t'), KeyModifiers::CONTROL);
        handle_key(&mut empty, KeyCode::Down, KeyModifiers::NONE);
        handle_key(&mut empty, KeyCode::Char('y'), KeyModifiers::NONE);
        assert_eq!(empty.tool_selected, 0);
        assert!(empty.clipboard.is_none());
    }

    #[test]
    fn tool_inspector_tracks_selection_when_old_calls_are_evicted() {
        let mut app = App::new();
        for index in 0..8 {
            app.apply(codeloom_core::event::LiveEvent::ToolStarted {
                call_id: index.to_string(),
                name: "view".into(),
            });
        }
        app.tools_open = true;
        app.tool_selected = 2;
        app.tool_scroll = (10, 5);
        for index in 8..10 {
            app.apply(codeloom_core::event::LiveEvent::ToolStarted {
                call_id: index.to_string(),
                name: "view".into(),
            });
            assert_eq!(app.tools[app.tool_selected].call_id, "2");
        }
        assert_eq!(app.tool_scroll, (10, 5));
        app.apply(codeloom_core::event::LiveEvent::ToolStarted {
            call_id: "10".into(),
            name: "view".into(),
        });
        assert_eq!(app.tools[app.tool_selected].call_id, "3");
        assert_eq!(app.tool_scroll, (0, 0));
        app.restore_messages(Vec::new());
        assert!(!app.tools_open);
        assert!(app.tools.is_empty());
    }

    #[test]
    fn queue_editing_preserves_original_and_composer_until_accepting() {
        let mut app = App::new();
        app.pending_prompts.push_back("original".into());
        app.queued = 1;
        app.composer.set_text("separate draft");
        handle_key(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert!(
            app.queue_editing,
            "empty prompts cannot replace the original"
        );
        handle_event(&mut app, Event::Paste("changed\nsecond line".into()));
        assert_eq!(app.pending_prompts[0], "original");
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(app.queue_open);
        assert!(!app.queue_editing);
        assert_eq!(app.pending_prompts[0], "original");
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
        handle_event(&mut app, Event::Paste("changed".into()));
        handle_key(&mut app, KeyCode::Char('j'), KeyModifiers::CONTROL);
        handle_key(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(app.pending_prompts[0], "changed\nx");
        assert_eq!(app.composer.text(), "separate draft");
        assert_eq!(app.queued, 1);
        assert!(!app.queue_editing);
    }

    #[test]
    fn queue_pause_cancel_and_resume_dispatch_one_prompt_at_a_time() {
        let mut app = App::new();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let token = CancellationToken::new();
        let active = Arc::new(Mutex::new(Some(token.clone())));
        app.turn_in_flight = true;
        send(&mut app, &tx, Some("first".into()));
        send(&mut app, &tx, Some("second".into()));
        handle_key(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
        handle_key(&mut app, KeyCode::Char('x'), KeyModifiers::CONTROL);
        cancel_active_turn(&mut app, &active);
        assert!(token.is_cancelled());
        assert!(app.queue_paused);
        app.apply(codeloom_core::event::LiveEvent::TurnEnded);
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        dispatch_next(&mut app, &tx);
        assert!(rx.try_recv().is_err());
        assert_eq!(app.queued, 2);

        handle_key(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
        handle_key(&mut app, KeyCode::Char(' '), KeyModifiers::NONE);
        dispatch_next(&mut app, &tx);
        assert!(
            rx.try_recv().is_err(),
            "inspection holds dispatch until close"
        );
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        dispatch_next(&mut app, &tx);
        assert_eq!(rx.try_recv().unwrap(), "first");
        dispatch_next(&mut app, &tx);
        assert!(rx.try_recv().is_err());
        app.apply(codeloom_core::event::LiveEvent::Error {
            message: "provider failed".into(),
        });
        dispatch_next(&mut app, &tx);
        assert_eq!(rx.try_recv().unwrap(), "second");
        assert_eq!(app.queued, 0);
        assert_eq!(app.queue_selected, 0);

        app.apply(codeloom_core::event::LiveEvent::TurnEnded);
        drop(rx);
        send(&mut app, &tx, Some("retained on failure".into()));
        assert_eq!(app.pending_prompts[0], "retained on failure");
        assert!(!app.turn_in_flight);
    }

    #[test]
    fn reverse_tab_restores_focus_and_dialogs_preserve_the_draft() {
        let mut app = App::new();
        app.composer.set_text("unfinished prompt");
        handle_key(&mut app, KeyCode::BackTab, KeyModifiers::SHIFT);
        assert_eq!(app.focus, Focus::Transcript);
        handle_key(&mut app, KeyCode::F(1), KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::BackTab, KeyModifiers::SHIFT);
        assert_eq!(app.focus, Focus::Transcript);
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Composer);
        assert_eq!(app.composer.text(), "unfinished prompt");
        assert!(!app.should_quit);
    }

    #[test]
    fn repeat_edits_text_but_does_not_submit_or_answer_permissions() {
        use crossterm::event::KeyEvent;
        let mut app = App::new();
        let repeat = |code| {
            Event::Key(KeyEvent::new_with_kind(
                code,
                KeyModifiers::NONE,
                KeyEventKind::Repeat,
            ))
        };
        handle_event(&mut app, repeat(KeyCode::Char('a')));
        handle_event(&mut app, repeat(KeyCode::Char('b')));
        handle_event(&mut app, repeat(KeyCode::Backspace));
        assert_eq!(app.composer.text(), "a");
        assert_eq!(handle_event(&mut app, repeat(KeyCode::Enter)), None);
        assert_eq!(app.composer.text(), "a");
        let mut reply = open_dialog(&mut app);
        handle_event(&mut app, repeat(KeyCode::Char('a')));
        handle_event(&mut app, repeat(KeyCode::Enter));
        assert!(matches!(
            reply.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
        assert!(app.permission.is_some());
    }

    fn ask_question_for_keys() -> codeloom_core::question::Question {
        use codeloom_core::question::{Question, QuestionOption};
        let option = |label: &str| QuestionOption {
            label: label.into(),
            description: String::new(),
        };
        Question {
            header: "Stack".into(),
            question: "Which?".into(),
            options: vec![option("a"), option("b")],
            multi_select: false,
        }
    }

    #[test]
    fn keys_reach_the_question_dialog_and_not_the_composer() {
        use codeloom_core::question::Answer;
        let mut app = App::new();
        let (tx, mut rx) = oneshot::channel();
        app.enqueue_questions(vec![ask_question_for_keys()], tx);
        handle_key(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::F(1), KeyModifiers::NONE);
        handle_event(&mut app, Event::Paste("oculto".into()));
        assert!(app.composer.is_empty());
        assert!(!app.help_open);
        assert!(app.question.is_some());
        handle_key(&mut app, KeyCode::Down, KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.question.is_none());
        assert_eq!(
            rx.try_recv().unwrap(),
            Some(vec![Answer {
                question: "Which?".into(),
                selected: vec!["b".into()],
            }])
        );

        // Esc responde a la pregunta (sin contar para la salida por doble Esc).
        let (tx, mut rx) = oneshot::channel();
        app.enqueue_questions(vec![ask_question_for_keys()], tx);
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(rx.try_recv().unwrap(), None);
        assert!(!app.should_quit);
        assert!(app.last_escape.is_none());
        assert!(app.composer.is_empty());
    }

    #[test]
    fn tab_moves_focus_and_transcript_keys_scroll() {
        let mut app = App::new();
        app.viewport.set_geometry(100, 10);
        handle_key(&mut app, KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Transcript);
        handle_key(&mut app, KeyCode::Char('k'), KeyModifiers::NONE);
        assert_eq!(app.composer.text(), "");
        assert!(!app.viewport.is_following());
        handle_key(&mut app, KeyCode::Char('G'), KeyModifiers::NONE);
        assert!(app.viewport.is_following());
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Transcript);
        assert!(
            app.should_quit,
            "Esc sale también con foco en el transcript"
        );
    }

    #[test]
    fn plain_c_types_and_enter_submits() {
        let mut app = App::new();
        assert_eq!(
            handle_key(&mut app, KeyCode::Char('h'), KeyModifiers::NONE),
            None
        );
        assert_eq!(
            handle_key(&mut app, KeyCode::Char('i'), KeyModifiers::NONE),
            None
        );
        assert_eq!(
            handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE),
            Some("hi".to_string())
        );
        assert!(!app.should_quit);
    }

    #[test]
    fn ctrl_j_adds_a_line_and_enter_sends() {
        let mut app = App::new();
        handle_key(&mut app, KeyCode::Char('a'), KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Char('j'), KeyModifiers::CONTROL);
        handle_key(&mut app, KeyCode::Char('b'), KeyModifiers::NONE);
        assert_eq!(app.composer.text(), "a\nb");
        let sent = handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(sent, Some("a\nb".to_string()));
    }

    #[test]
    fn up_recalls_history_when_single_line() {
        let mut app = App::new();
        for c in "hola".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::NONE);
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(app.composer.text(), "hola");
    }

    #[test]
    fn escape_quits_with_transcript_selection() {
        let mut app = App::new();
        for s in ["uno", "dos", "tres"] {
            app.push_user(s);
        }
        handle_key(&mut app, KeyCode::Tab, KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Char('v'), KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Char('k'), KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Char('y'), KeyModifiers::NONE);
        assert_eq!(app.clipboard.as_deref(), Some("dos\ntres"));
        assert!(app.selection.is_none());
        handle_key(&mut app, KeyCode::Char('v'), KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(app.should_quit);
        assert_eq!(app.focus, Focus::Transcript);
    }

    #[test]
    fn up_moves_the_cursor_when_multiline() {
        let mut app = App::new();
        app.composer.set_text("uno\ndos");
        handle_key(&mut app, KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(app.composer.cursor_row_col().0, 0);
    }

    #[test]
    fn budget_exhausted_stops_waking_until_user_writes() {
        use codeloom_core::session::{SessionId, SubagentStatus};
        let mut app = App::new();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        app.session_id = Some(SessionId(1));
        app.auto_turns_left = 0;
        app.apply(codeloom_core::event::LiveEvent::SubagentSettled {
            parent: SessionId(1),
            child: SessionId(9),
            status: SubagentStatus::Completed,
        });

        assert!(!app.take_wake());
        assert_eq!(app.status, "auto-turns exhausted — write to continue");
        assert!(!app.wake_pending);
        assert!(!app.wake_in_flight);
        assert_eq!(app.auto_turns_left, 0);

        // Solo un mensaje del usuario rearma el presupuesto.
        send(&mut app, &tx, Some("hola".into()));
        assert_eq!(app.auto_turns_left, crate::app::MAX_AUTO_TURNS);
        assert_eq!(rx.try_recv().unwrap(), "hola");
    }

    #[test]
    fn queued_prompt_waits_while_a_wake_is_in_flight() {
        let mut app = App::new();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        app.wake_in_flight = true;
        send(&mut app, &tx, Some("después".into()));
        assert!(
            rx.try_recv().is_err(),
            "no se solapa un turno con el despertar en vuelo"
        );
        assert_eq!(app.pending_prompts.len(), 1);

        app.apply(codeloom_core::event::LiveEvent::WakeIdle);
        dispatch_next(&mut app, &tx);
        assert_eq!(rx.try_recv().unwrap(), "después");
    }

    #[test]
    fn dispatch_wake_releases_the_flag_when_the_worker_is_gone() {
        let mut app = App::new();
        app.wake_pending = true;
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<()>();
        drop(rx);
        dispatch_wake(&mut app, &tx);
        assert!(!app.wake_in_flight);
        assert_eq!(app.status, "error: wake channel closed");
    }

    #[test]
    fn cancel_clears_pending_wake() {
        use codeloom_core::session::{SessionId, SubagentStatus};
        let mut app = App::new();
        app.session_id = Some(SessionId(1));
        app.turn_in_flight = true;
        app.apply(codeloom_core::event::LiveEvent::SubagentSettled {
            parent: SessionId(1),
            child: SessionId(9),
            status: SubagentStatus::Completed,
        });
        assert!(app.wake_pending);

        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(!app.wake_pending);
        app.apply(codeloom_core::event::LiveEvent::TurnEnded);
        assert!(
            !app.take_wake(),
            "cancelar no rearma el despertar del mismo evento"
        );
    }

    #[test]
    fn crush_and_classic_commands_switch_the_look() {
        use batten::widgets::{Look, ThemePreset};
        let mut app = App::new();
        app.composer.set_text("/crush");
        assert_eq!(
            handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE),
            None
        );
        assert_eq!(app.theme_preset, ThemePreset::Crush);
        assert_eq!(app.theme.look, Look::crush());
        assert_eq!(
            app.slash_feedback.as_deref(),
            Some("Theme: Crush · Charmtone")
        );

        app.composer.set_text("/classic");
        assert_eq!(
            handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE),
            None
        );
        assert_eq!(app.theme_preset, ThemePreset::Batten);
        assert_eq!(app.theme.look, Look::classic());
        assert_eq!(app.slash_feedback.as_deref(), Some("Theme: Batten"));
    }

    #[test]
    fn mouse_command_toggles_capture_and_reports() {
        let mut app = App::new();
        assert!(app.mouse_capture);
        app.composer.set_text("/mouse");
        assert_eq!(
            handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE),
            None
        );
        assert!(!app.mouse_capture);
        assert!(app.mouse_capture_dirty);
        assert_eq!(
            app.slash_feedback.as_deref(),
            Some("Mouse capture off — select text with the mouse")
        );
        assert!(app.composer.is_empty());

        app.composer.set_text("/mouse");
        assert_eq!(
            handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE),
            None
        );
        assert!(app.mouse_capture);
        assert_eq!(
            app.slash_feedback.as_deref(),
            Some("Mouse capture on (wheel scrolls)")
        );
    }

    #[test]
    fn esc_returns_focus_from_side_panes_to_the_composer() {
        let mut app = App::new();
        app.focus = Focus::Sidebar;
        assert_eq!(handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE), None);
        assert_eq!(app.focus, Focus::Composer);
        // Esc que solo devuelve el foco no cuenta para la doble Esc de salida.
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(!app.should_quit);
    }

    #[test]
    fn printable_key_in_a_side_pane_refocuses_and_types() {
        let mut app = App::new();
        app.focus = Focus::Agent;
        handle_key(&mut app, KeyCode::Char('h'), KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Composer);
        assert_eq!(app.composer.text(), "h");
    }

    #[test]
    fn printable_key_with_a_dialog_open_does_not_refocus() {
        let mut app = App::new();
        app.focus = Focus::Activity;
        let _rx = open_dialog(&mut app);
        handle_key(&mut app, KeyCode::Char('z'), KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Activity);
        assert!(app.composer.is_empty());
    }

    #[test]
    fn tab_still_cycles_composer_and_transcript_only() {
        let mut app = App::new();
        handle_key(&mut app, KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Transcript);
        handle_key(&mut app, KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Composer);
        // Desde un panel lateral, Tab vuelve al composer en vez de entrar en él.
        app.focus = Focus::Rail;
        handle_key(&mut app, KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Composer);
    }
}
