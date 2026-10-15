# Demo `chat` con guion y streaming — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Que la demo `chat` del showcase reproduzca un turno con tool y respuesta por deltas, y un turno con permiso, con pausa/paso/reinicio.

**Architecture:** Un guion (`VecDeque<Step>`) que avanza un paso cada 3 ticks; el texto en curso vive en `Stream` y se pinta sobre una copia del transcript; el diálogo de permisos bloquea el guion hasta que se resuelve.

**Tech Stack:** Rust, `ratatui` 0.30, batten.

**Spec:** §5 de `docs/specs/2026-10-15-batten-reference-findings-showcase.md` (primera entrega); diseño acordado en conversación (2026-10-15).

## Global Constraints

- Comentarios en español. Mensaje de commit: el indicado, tal cual.
- El commit pasa: `cargo build`, `cargo test`, `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`.
- **Entorno (Git Bash en Windows):** antes de cualquier `cargo`, en el mismo comando:
  `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe'`.
  Si el enlazado falla con `LNK1104`, hay un ejecutable abierto: reportarlo, no matar procesos.
- No hacer `git push`. No tocar ficheros fuera de los listados. No editar Rust con scripts.

## Review Focus

- Con el diálogo abierto el guion no avanza. Test en Task 1.
- Denegar cancela la tool y descarta el resto del guion. Test en Task 1.
- La respuesta cerrada es exactamente la suma de los deltas. Test en Task 1.
- Nota: el showcase intercepta `Esc` (vuelve al índice), así que en la demo se deniega con `d`.

---

### Task 1: guion en la demo `chat`

**Files:**
- Modify: `crates/batten/src/state/transcript.rs`, `crates/batten/examples/showcase/demos/chat.rs`

- [ ] **Step 1: `Transcript` clonable**

En `crates/batten/src/state/transcript.rs`, cambiar `#[derive(Debug, Default)]` (el que está justo encima de `pub struct Transcript {`) por `#[derive(Debug, Default, Clone)]`.

- [ ] **Step 2: Reescribir `crates/batten/examples/showcase/demos/chat.rs` entero**

```rust
//! Demo de la composición completa con un guion determinista: turno normal
//! (tool + respuesta por deltas) y turno con permiso.

use crate::demo::Demo;
use batten::state::{
    Composer, Line, LineKind, PermissionBody, PermissionChoice, PermissionDialog, Sidebar,
    Stream, ToolItem, Transcript, Viewport,
};
use batten::widgets::{composer, permission, sidebar, tool_item, transcript, Anim, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::Frame;
use std::collections::VecDeque;

/// Ticks entre pasos del guion (~150 ms con el reloj de 50 ms).
const TICKS_PER_STEP: u32 = 3;

const ANSWER: &str = "He leído el fichero y todo está en orden. El transcript ahora cuenta \
filas visuales, así que las líneas largas como esta se parten en varias filas sin descuadrar \
el scroll ni la barra. Si subes con PgUp mientras escribo, la vista se queda quieta; al volver \
abajo con PgDn, vuelve a seguir el final. Prueba también F6 para un turno que pide permiso.";

const PERMISSION_ANSWER: &str = "Hecho: target/ limpio.";

#[derive(Debug, Clone, PartialEq)]
enum Step {
    ToolStart(&'static str),
    ToolDone,
    AwaitPermission(&'static str),
    Delta(String),
    Flush,
}

fn answer_steps(text: &str) -> Vec<Step> {
    text.split_inclusive(' ')
        .map(|w| Step::Delta(w.to_string()))
        .chain([Step::Flush])
        .collect()
}

pub struct ChatDemo {
    transcript: Transcript,
    viewport: Viewport,
    composer: Composer,
    sidebar: Sidebar,
    tool: Option<ToolItem>,
    stream: Stream,
    script: VecDeque<Step>,
    dialog: Option<PermissionDialog>,
    anim: Anim,
    paused: bool,
    wait: u32,
}

impl ChatDemo {
    pub fn new() -> Self {
        let mut sidebar = Sidebar::with_logo("batten");
        sidebar.push_section("estado", vec!["demo".into()]);
        sidebar.push_section("modelo", vec!["simulado".into()]);
        Self {
            transcript: Transcript::new(),
            viewport: Viewport::new(),
            composer: Composer::new(),
            sidebar,
            tool: None,
            stream: Stream::new(),
            script: VecDeque::new(),
            dialog: None,
            anim: Anim::new(),
            paused: false,
            wait: 0,
        }
    }

    /// Turno normal: tool `view` y respuesta palabra a palabra.
    fn start_turn(&mut self, prompt: String) {
        self.transcript.push(Line::new(LineKind::User, prompt));
        self.viewport.follow_bottom();
        self.script
            .extend([Step::ToolStart("view"), Step::ToolDone]);
        self.script.extend(answer_steps(ANSWER));
    }

    /// Turno con permiso: `bash cargo clean` espera la decisión del usuario.
    fn start_permission_turn(&mut self) {
        self.transcript
            .push(Line::new(LineKind::User, "limpia el build"));
        self.viewport.follow_bottom();
        self.script.extend([
            Step::ToolStart("bash"),
            Step::AwaitPermission("cargo clean"),
            Step::ToolDone,
        ]);
        self.script.extend(answer_steps(PERMISSION_ANSWER));
    }

    /// Ejecuta el siguiente paso. No avanza mientras haya un diálogo abierto.
    fn step(&mut self) {
        if self.dialog.is_some() {
            return;
        }
        let Some(step) = self.script.pop_front() else {
            return;
        };
        match step {
            Step::ToolStart(name) => {
                let mut t = ToolItem::new("demo", name);
                t.start();
                self.tool = Some(t);
            }
            Step::ToolDone => {
                if let Some(t) = self.tool.as_mut() {
                    t.set_result(false);
                }
            }
            Step::AwaitPermission(cmd) => {
                if let Some(t) = self.tool.as_mut() {
                    t.await_permission();
                }
                self.dialog = Some(PermissionDialog::new(
                    "bash",
                    "",
                    PermissionBody::Text(cmd.into()),
                ));
            }
            Step::Delta(s) => self.stream.push_text_delta(&s),
            Step::Flush => self.stream.flush(&mut self.transcript),
        }
    }

    /// Cierra el diálogo: permitir sigue el guion; denegar cancela y lo descarta.
    fn resolve(&mut self, choice: PermissionChoice) {
        self.dialog = None;
        if choice == PermissionChoice::Deny {
            if let Some(t) = self.tool.as_mut() {
                t.cancel();
            }
            self.script.clear();
            self.transcript
                .push(Line::new(LineKind::System, "permiso denegado"));
        } else if let Some(t) = self.tool.as_mut() {
            t.resolve_awaiting();
        }
    }

    /// Transcript con la respuesta en curso añadida al final.
    fn view(&self) -> Transcript {
        let mut t = self.transcript.clone();
        if !self.stream.is_empty() {
            t.push(Line::new(LineKind::Assistant, self.stream.pending()));
        }
        t
    }
}

impl Demo for ChatDemo {
    fn title(&self) -> &str {
        "chat"
    }

    fn hint(&self) -> &str {
        "Enter: turno · F6: con permiso · F2 reinicia · F3 pausa · F4 paso · PgUp/PgDn"
    }

    fn tick(&mut self, anim: &Anim) {
        self.anim = *anim;
        if self.paused {
            return;
        }
        self.wait += 1;
        if self.wait >= TICKS_PER_STEP {
            self.wait = 0;
            self.step();
        }
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let wide = area.width >= 60;
        let (body, side) = if wide {
            let cols = Layout::horizontal([Constraint::Min(1), Constraint::Length(28)]).split(area);
            (cols[0], Some(cols[1]))
        } else {
            (area, None)
        };
        let rows = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(body);
        if let Some(t) = &self.tool {
            tool_item::render(f, rows[0], t, &self.anim, theme);
        }
        let view = self.view();
        transcript::render(f, rows[1], &view, &mut self.viewport, theme);
        composer::render(f, rows[2], &self.composer, theme);
        if let Some(s) = side {
            sidebar::render(f, s, &self.sidebar, theme);
        }
        if let Some(d) = &self.dialog {
            permission::render(f, area, d, theme);
        }
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        if let Some(d) = self.dialog.as_mut() {
            let choice = match code {
                KeyCode::Left => {
                    d.prev();
                    None
                }
                KeyCode::Right => {
                    d.next();
                    None
                }
                KeyCode::Up => {
                    d.scroll_by(-1);
                    None
                }
                KeyCode::Down => {
                    d.scroll_by(1);
                    None
                }
                KeyCode::Enter => Some(d.choice()),
                KeyCode::Char('a') => Some(PermissionChoice::Allow),
                KeyCode::Char('s') => Some(PermissionChoice::AllowSession),
                KeyCode::Char('d') => Some(PermissionChoice::Deny),
                _ => None,
            };
            if let Some(c) = choice {
                self.resolve(c);
            }
            return true;
        }
        match code {
            KeyCode::Enter => {
                if let Some(text) = self.composer.submit() {
                    self.start_turn(text);
                }
            }
            KeyCode::F(6) => self.start_permission_turn(),
            KeyCode::F(2) => *self = Self::new(),
            KeyCode::F(3) => self.paused = !self.paused,
            KeyCode::F(4) => self.step(),
            KeyCode::PageUp => self.viewport.scroll_up(5),
            KeyCode::PageDown => self.viewport.scroll_down(5),
            KeyCode::Backspace => self.composer.backspace(),
            KeyCode::Char(c) => self.composer.push_char(c),
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use batten::state::ToolStatus;

    fn ticks(d: &mut ChatDemo, n: u32) {
        let a = Anim::new();
        for _ in 0..n {
            d.tick(&a);
        }
    }

    fn type_and_send(d: &mut ChatDemo, text: &str) {
        for c in text.chars() {
            d.handle_key(KeyCode::Char(c));
        }
        d.handle_key(KeyCode::Enter);
    }

    #[test]
    fn enter_starts_a_turn_that_streams_and_flushes() {
        let mut d = ChatDemo::new();
        type_and_send(&mut d, "hi");
        assert_eq!(d.transcript.line(0).unwrap().plain(), "hi");
        ticks(&mut d, TICKS_PER_STEP * 5); // tool start, tool done, 3 deltas
        assert!(!d.stream.is_empty());
        assert_eq!(d.transcript.len(), 1);
        assert_eq!(d.view().len(), 2);
        ticks(&mut d, TICKS_PER_STEP * 200);
        assert!(d.stream.is_empty());
        assert_eq!(d.transcript.len(), 2);
        assert_eq!(d.transcript.line(1).unwrap().plain(), ANSWER);
        assert_eq!(d.tool.as_ref().unwrap().status(), ToolStatus::Success);
    }

    #[test]
    fn permission_turn_blocks_until_allowed() {
        let mut d = ChatDemo::new();
        d.handle_key(KeyCode::F(6));
        ticks(&mut d, TICKS_PER_STEP * 2); // tool start, await permission
        assert!(d.dialog.is_some());
        let pending = d.script.len();
        ticks(&mut d, TICKS_PER_STEP * 10);
        assert_eq!(d.script.len(), pending, "el guion no avanza con el diálogo abierto");
        d.handle_key(KeyCode::Char('a'));
        assert!(d.dialog.is_none());
        ticks(&mut d, TICKS_PER_STEP * 20);
        assert_eq!(d.tool.as_ref().unwrap().status(), ToolStatus::Success);
        assert_eq!(d.transcript.line(1).unwrap().plain(), PERMISSION_ANSWER);
    }

    #[test]
    fn denying_cancels_and_drops_the_script() {
        let mut d = ChatDemo::new();
        d.handle_key(KeyCode::F(6));
        ticks(&mut d, TICKS_PER_STEP * 2);
        d.handle_key(KeyCode::Char('d'));
        assert_eq!(d.tool.as_ref().unwrap().status(), ToolStatus::Canceled);
        assert!(d.script.is_empty());
        let last = d.transcript.line(d.transcript.len() - 1).unwrap();
        assert_eq!(last.kind, LineKind::System);
        assert_eq!(last.plain(), "permiso denegado");
    }

    #[test]
    fn pause_stops_and_f4_steps() {
        let mut d = ChatDemo::new();
        type_and_send(&mut d, "x");
        d.handle_key(KeyCode::F(3));
        ticks(&mut d, TICKS_PER_STEP * 10);
        assert!(d.tool.is_none());
        d.handle_key(KeyCode::F(4));
        assert!(d.tool.is_some());
    }

    #[test]
    fn f2_resets_the_scenario() {
        let mut d = ChatDemo::new();
        type_and_send(&mut d, "x");
        d.handle_key(KeyCode::F(2));
        assert!(d.transcript.is_empty());
        assert!(d.script.is_empty());
    }
}
```

- [ ] **Step 3: Checks**

Run: `cargo build && cargo test && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (si `fmt --check` falla, `cargo fmt` y repetir). Si `ToolStatus` no implementa `PartialEq`/`Debug`, sustituir esos `assert_eq!` por `assert!(matches!(d.tool.as_ref().unwrap().status(), ToolStatus::Success))` (o `Canceled`) y reportarlo.

- [ ] **Step 4: Commit**

```bash
git add crates/batten/src/state/transcript.rs crates/batten/examples/showcase/demos/chat.rs
git commit -m "feat(batten): demo chat con guion, streaming por deltas y permiso"
```
