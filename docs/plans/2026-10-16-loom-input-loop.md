# Bucle de entrada de loom — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Entrada sin lag ni bloqueos: bucle por eventos (`EventStream` + `select!`), ráfagas agrupadas en un frame, pegado entre corchetes que nunca envía, frames sincronizados, sin redibujar sin cambios y sin perder prompts.

**Architecture:** `frame::FrameScheduler` (puro) decide cuándo pintar (`Input` ya; `Background` ≤ 60 fps). `run.rs` orquesta IO con `tokio::select!` sobre teclado, permisos, bus, vencimiento de frame y tick de animación. `App` gana `paste` y un contador de prompts en cola. El canal de prompts pasa a ilimitado.

**Tech Stack:** Rust, `crossterm` 0.29 (`event-stream`, `bracketed-paste`), `futures`, `tokio`, `ratatui` 0.30.

**Spec:** `docs/specs/2026-10-16-loom-input-loop-design.md`

## Global Constraints

- `codeloom-core` sin ratatui/crossterm; batten sin crossterm (este plan no toca batten).
- Comentarios en español. Mensajes de commit: los indicados, tal cual.
- Cada commit pasa: `cargo test -p codeloom-tui`, `cargo test -p codeloom-core`, `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`.
- **Entorno (Git Bash en Windows):** antes de cualquier `cargo`, en el mismo comando:
  `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe'`.
  No ejecutar `cargo build`/`cargo test` sin `-p` (el usuario puede tener `loom.exe` abierto). No matar procesos. No crear directorios de target temporales.
- No hacer `git push`. No tocar ficheros fuera de los listados. No editar Rust con scripts.

## Review Focus

- `Input` siempre pinta ya, aunque haya un `Background` reciente. Test en Task 1.
- Pegado con `\r\n`/`\r` no envía; ignorado con diálogo abierto o foco en transcript. Test en Task 2.
- El contador de cola no baja de 0. Test en Task 2.
- El bus cerrado o `Lagged` no hace girar el bucle en vacío (los brazos se desactivan). Revisión de código en Task 3.

---

### Task 1: `FrameScheduler`

**Files:**
- Create: `crates/codeloom-tui/src/frame.rs`
- Modify: `crates/codeloom-tui/src/lib.rs`

**Interfaces:**
- Produces: `frame::{Urgency { Input, Background }, FrameScheduler::{new(Duration), request(Urgency), pending() -> bool, due(Instant) -> bool, deadline() -> Option<Instant>, drawn(Instant)}, MIN_FRAME: Duration}`.

- [ ] **Step 1: Crear `crates/codeloom-tui/src/frame.rs`**

```rust
//! Planificador de frames: la entrada se pinta ya; el resto (streaming,
//! animaciones) como mucho cada `min_interval`. Sin peticiones no se pinta.

use std::time::{Duration, Instant};

/// Intervalo mínimo entre frames de fondo (~60 fps).
pub const MIN_FRAME: Duration = Duration::from_millis(16);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    /// Teclado, pegado, resize, diálogos: se pinta en cuanto se procesa.
    Input,
    /// Eventos del agente y animaciones: se agrupan.
    Background,
}

#[derive(Debug)]
pub struct FrameScheduler {
    pending: Option<Urgency>,
    last: Option<Instant>,
    min_interval: Duration,
}

impl FrameScheduler {
    pub fn new(min_interval: Duration) -> Self {
        Self {
            pending: None,
            last: None,
            min_interval,
        }
    }

    /// Pide un frame; `Input` gana a `Background`.
    pub fn request(&mut self, urgency: Urgency) {
        self.pending = match (self.pending, urgency) {
            (Some(Urgency::Input), _) | (_, Urgency::Input) => Some(Urgency::Input),
            _ => Some(Urgency::Background),
        };
    }

    pub fn pending(&self) -> bool {
        self.pending.is_some()
    }

    /// ¿Hay que pintar ya?
    pub fn due(&self, now: Instant) -> bool {
        match self.pending {
            None => false,
            Some(Urgency::Input) => true,
            Some(Urgency::Background) => self
                .last
                .is_none_or(|last| now.duration_since(last) >= self.min_interval),
        }
    }

    /// Cuándo vence un frame de fondo pendiente (para esperar sin girar).
    pub fn deadline(&self) -> Option<Instant> {
        match self.pending {
            Some(Urgency::Background) => self.last.map(|last| last + self.min_interval),
            _ => None,
        }
    }

    /// Se acaba de pintar.
    pub fn drawn(&mut self, now: Instant) {
        self.pending = None;
        self.last = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sched() -> (FrameScheduler, Instant) {
        (FrameScheduler::new(MIN_FRAME), Instant::now())
    }

    #[test]
    fn nothing_pending_is_never_due() {
        let (s, t0) = sched();
        assert!(!s.pending());
        assert!(!s.due(t0));
        assert_eq!(s.deadline(), None);
    }

    #[test]
    fn input_is_due_immediately_even_right_after_a_frame() {
        let (mut s, t0) = sched();
        s.drawn(t0);
        s.request(Urgency::Input);
        assert!(s.due(t0));
    }

    #[test]
    fn background_waits_for_the_interval() {
        let (mut s, t0) = sched();
        s.drawn(t0);
        s.request(Urgency::Background);
        assert!(!s.due(t0 + Duration::from_millis(5)));
        assert_eq!(s.deadline(), Some(t0 + MIN_FRAME));
        assert!(s.due(t0 + MIN_FRAME));
    }

    #[test]
    fn first_background_frame_is_due() {
        let (mut s, t0) = sched();
        s.request(Urgency::Background);
        assert!(s.due(t0));
    }

    #[test]
    fn input_upgrades_a_pending_background() {
        let (mut s, t0) = sched();
        s.drawn(t0);
        s.request(Urgency::Background);
        s.request(Urgency::Input);
        assert!(s.due(t0));
        assert_eq!(s.deadline(), None);
        s.request(Urgency::Background);
        assert!(s.due(t0), "Background no rebaja un Input pendiente");
    }

    #[test]
    fn drawn_clears_the_request() {
        let (mut s, t0) = sched();
        s.request(Urgency::Input);
        s.drawn(t0);
        assert!(!s.pending());
    }
}
```

- [ ] **Step 2: Registrar en `lib.rs`**

Añadir `pub mod frame;` después de `pub mod clipboard;`.

- [ ] **Step 3: Checks**

Run: `cargo test -p codeloom-tui && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS. Si `Option::is_none_or` no existe en el toolchain, sustituirlo por `.map_or(true, |last| now.duration_since(last) >= self.min_interval)` y reportarlo (si clippy pide lo contrario, seguir a clippy).

- [ ] **Step 4: Commit**

```bash
git add crates/codeloom-tui/src
git commit -m "feat(tui): planificador de frames por urgencia"
```

---

### Task 2: pegado, cola de prompts y `handle_event`

**Files:**
- Modify: `crates/codeloom-tui/src/app.rs`, `crates/codeloom-tui/src/render.rs`, `crates/codeloom-tui/src/run.rs`

**Interfaces:**
- Produces: `App::paste(&mut self, &str)`, `App.queued: usize`; `run::handle_event(&mut App, crossterm::event::Event) -> Option<String>`.

- [ ] **Step 1: `app.rs`**

1. En `pub struct App`, después de `pub clipboard: Option<String>,`, añadir:
   ```rust
       /// Prompts enviados que el agente aún no ha empezado.
       pub queued: usize,
   ```
   y en `App::new`, después de `clipboard: None,`, añadir `queued: 0,`.
2. En `apply`, sustituir `LiveEvent::TurnStarted => self.status = "working".into(),` por:
   ```rust
            LiveEvent::TurnStarted => {
                self.status = "working".into();
                self.queued = self.queued.saturating_sub(1);
            }
   ```
3. En `impl App`, añadir:
   ```rust
       /// Texto pegado (bracketed paste): entra de una vez en el composer y
       /// nunca envía. Se ignora con un diálogo abierto o sin foco en el composer.
       pub fn paste(&mut self, text: &str) {
           if self.permission.is_some() || self.focus != Focus::Composer {
               return;
           }
           let text = text.replace("\r\n", "\n").replace('\r', "\n");
           self.composer.insert_str(&text);
           self.history.reset();
       }
   ```
4. En `mod tests`, añadir:
   ```rust
       #[test]
       fn paste_inserts_normalized_text_without_sending() {
           let mut app = App::new();
           app.paste("a\r\nb\rc");
           assert_eq!(app.composer.text(), "a\nb\nc");
       }

       #[test]
       fn paste_is_ignored_without_composer_focus() {
           let mut app = App::new();
           app.toggle_focus();
           app.paste("x");
           assert_eq!(app.composer.text(), "");
       }

       #[test]
       fn queue_counter_drops_on_turn_start_and_never_underflows() {
           let mut app = App::new();
           app.queued = 2;
           app.apply(codeloom_core::event::LiveEvent::TurnStarted);
           assert_eq!(app.queued, 1);
           app.apply(codeloom_core::event::LiveEvent::TurnStarted);
           app.apply(codeloom_core::event::LiveEvent::TurnStarted);
           assert_eq!(app.queued, 0);
       }
   ```

- [ ] **Step 2: Cabecera con la cola (`render.rs`)**

En `draw_header`, sustituir `let header = Line::from(format!(" CodeLoom · {} ", app.status));` por:

```rust
    let queue = match app.queued {
        0 => String::new(),
        n => format!(" · en cola ({n})"),
    };
    let header = Line::from(format!(" CodeLoom · {}{queue} ", app.status));
```

y en `mod tests` añadir:

```rust
    #[test]
    fn header_shows_queued_prompts() {
        let mut app = App::new();
        app.queued = 2;
        assert!(buffer_of(&mut app, 60, 10).contains("en cola (2)"));
    }
```

- [ ] **Step 3: `handle_event` (`run.rs`)**

1. Cambiar `use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};` por `use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};` (sin cambios todavía; `event` se dejará de usar en la Task 3).
2. Después de `pub fn handle_key(...) { ... }`, añadir:
   ```rust
   /// Aplica un evento de terminal. Devuelve el prompt a enviar, si lo hay.
   pub fn handle_event(app: &mut App, ev: Event) -> Option<String> {
       match ev {
           Event::Key(k) if k.kind == KeyEventKind::Press => handle_key(app, k.code, k.modifiers),
           Event::Paste(text) => {
               app.paste(&text);
               None
           }
           _ => None,
       }
   }
   ```
3. En `mod tests`, añadir:
   ```rust
       #[test]
       fn pasted_newlines_never_submit() {
           let mut app = App::new();
           let sent = handle_event(&mut app, Event::Paste("uno\ndos\n".into()));
           assert_eq!(sent, None);
           assert_eq!(app.composer.text(), "uno\ndos\n");
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
   ```

- [ ] **Step 4: Checks**

Run: `cargo test -p codeloom-tui && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/codeloom-tui/src
git commit -m "feat(tui): pegado sin envío, cola de prompts y eventos de terminal"
```

---

### Task 3: bucle por eventos

**Files:**
- Modify: `crates/codeloom-tui/Cargo.toml`, `crates/codeloom-tui/src/run.rs`, `crates/codeloom-cli/src/main.rs`

- [ ] **Step 1: Dependencias (`crates/codeloom-tui/Cargo.toml`)**

Cambiar `crossterm = "0.29"` por:

```toml
crossterm = { version = "0.29", features = ["event-stream", "bracketed-paste"] }
```

y añadir `futures.workspace = true`.

- [ ] **Step 2: Imports y `RawGuard` (`run.rs`)**

1. Sustituir los `use` de la cabecera (desde `use crate::app::App;` hasta `use std::time::Duration;`) por:
   ```rust
   use crate::app::App;
   use crate::app::Focus;
   use crate::frame::{FrameScheduler, Urgency, MIN_FRAME};
   use crate::render::render;
   use crossterm::event::{
       DisableBracketedPaste, EnableBracketedPaste, Event, EventStream, KeyCode, KeyEventKind,
       KeyModifiers,
   };
   use crossterm::execute;
   use crossterm::terminal::{
       disable_raw_mode, enable_raw_mode, BeginSynchronizedUpdate, EndSynchronizedUpdate,
       EnterAlternateScreen, LeaveAlternateScreen,
   };
   use futures::{FutureExt, StreamExt};
   use ratatui::backend::CrosstermBackend;
   use ratatui::Terminal;
   use std::time::Instant;
   use tokio::sync::broadcast::error::RecvError;
   use tokio::sync::mpsc::UnboundedSender;

   /// Eventos de teclado que se procesan como máximo antes de pintar.
   const MAX_BURST: usize = 512;
   ```
2. En `RawGuard::enter`, cambiar `execute!(std::io::stdout(), EnterAlternateScreen)?;` por `execute!(std::io::stdout(), EnterAlternateScreen, EnableBracketedPaste)?;`.
3. En `Drop for RawGuard`, sustituir `let _ = std::io::stdout().execute(LeaveAlternateScreen);` por `let _ = execute!(std::io::stdout(), DisableBracketedPaste, LeaveAlternateScreen);`.

- [ ] **Step 3: Sustituir la función `run` entera por**

```rust
/// Envía un prompt al agente y lo cuenta como en cola.
fn send(app: &mut App, tx: &UnboundedSender<String>, prompt: Option<String>) {
    if let Some(text) = prompt {
        app.queued += 1;
        let _ = tx.send(text);
    }
}

/// Bucle principal por eventos: teclado (prioritario), permisos, bus del
/// agente, vencimiento de frame y animaciones. Solo pinta cuando hay cambios;
/// las ráfagas de teclado se agrupan en un frame; cada frame es atómico.
pub async fn run(
    mut app: App,
    prompt_tx: UnboundedSender<String>,
    mut bus_rx: tokio::sync::broadcast::Receiver<codeloom_core::event::LiveEvent>,
    mut perm_rx: tokio::sync::mpsc::Receiver<(
        codeloom_core::permission::PermissionRequest,
        crate::permission_flow::PermissionReply,
    )>,
) -> std::io::Result<()> {
    let _guard = RawGuard::enter()?;
    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend)?;
    let mut input = EventStream::new();
    let mut frames = FrameScheduler::new(MIN_FRAME);
    let mut anim = tokio::time::interval(batten::widgets::FRAME_INTERVAL);
    anim.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut fx: Option<batten::widgets::fx::Effect> = None;
    let mut fx_clock = Instant::now();
    let (mut bus_open, mut perm_open) = (true, true);
    frames.request(Urgency::Input);

    while !app.should_quit {
        let animating = fx.is_some() && app.permission.is_some();
        let deadline = frames.deadline();
        tokio::select! {
            biased;
            ev = input.next() => match ev {
                Some(Ok(ev)) => {
                    let prompt = handle_event(&mut app, ev);
                    send(&mut app, &prompt_tx, prompt);
                    for _ in 0..MAX_BURST {
                        match input.next().now_or_never() {
                            Some(Some(Ok(ev))) => {
                                let prompt = handle_event(&mut app, ev);
                                send(&mut app, &prompt_tx, prompt);
                            }
                            _ => break,
                        }
                    }
                    frames.request(Urgency::Input);
                }
                Some(Err(e)) => return Err(e),
                None => break,
            },
            req = perm_rx.recv(), if perm_open => match req {
                Some((req, reply)) => {
                    app.enqueue_permission(req, reply);
                    frames.request(Urgency::Input);
                }
                None => perm_open = false,
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
                frames.request(Urgency::Background);
            }
            _ = tokio::time::sleep_until(tokio::time::Instant::from_std(
                deadline.unwrap_or_else(Instant::now),
            )), if deadline.is_some() => {}
            _ = anim.tick(), if animating => frames.request(Urgency::Background),
        }

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

        let now = Instant::now();
        if frames.due(now) {
            let elapsed = now.duration_since(fx_clock);
            fx_clock = now;
            execute!(std::io::stdout(), BeginSynchronizedUpdate)?;
            terminal.draw(|f| {
                render(f, &mut app);
                if let (Some(e), Some(_)) = (fx.as_mut(), app.permission.as_ref()) {
                    let area = batten::widgets::permission::area(f.area());
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
    Ok(())
}
```

Si `elapsed.into()` no compila (no hay `From<std::time::Duration>` para `FxDuration`), usar `batten::widgets::fx::FxDuration::from_millis(elapsed.as_millis().min(u32::MAX as u128) as u32)` y reportarlo.

- [ ] **Step 4: CLI (`crates/codeloom-cli/src/main.rs`)**

Cambiar `let (prompt_tx, mut prompt_rx) = tokio::sync::mpsc::channel::<String>(8);` por
`let (prompt_tx, mut prompt_rx) = tokio::sync::mpsc::unbounded_channel::<String>();`. El resto (`prompt_rx.recv().await`, la llamada a `run`) no cambia.

- [ ] **Step 5: Checks**

Run: `cargo test -p codeloom-tui && cargo test -p codeloom-core && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (clippy compila también `codeloom-cli`). Quitar imports que queden sin uso si el compilador lo indica.

- [ ] **Step 6: Commit**

```bash
git add crates/codeloom-tui crates/codeloom-cli/src/main.rs Cargo.lock
git commit -m "feat(tui): bucle por eventos sin bloqueos con frames sincronizados"
```

- [ ] **Step 7: Verificación manual (la hace el usuario)**

`cargo run -p codeloom-cli --bin loom`: teclear rápido sin lag; pegar un bloque con saltos de línea (queda en el composer, no se envía); enviar dos prompts seguidos (`en cola (1)`); abrir un diálogo de permisos (efecto fluido).
