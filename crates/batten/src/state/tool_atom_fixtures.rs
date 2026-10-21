//! Datos de ejemplo de las herramientas reales de CodeLoom: una llamada
//! (`ToolItem`) y sus hechos (`ToolFacts`) por herramienta, tal como los dejaría
//! el adaptador `tool_facts` de `codeloom-tui`.
//!
//! Los consumen el catálogo «CodeLoom tools» del showcase y los tests de esta
//! pieza. Solo se compila con `cfg(test)` o con la feature `fixtures`, que el
//! propio crate activa para sus ejemplos mediante un dev-dependency sobre sí
//! mismo: así `cargo run -p batten --example showcase` funciona sin flags.

use super::tool_atom::{tool_lines_with, ToolFacts};
use super::{todo_lines, Line, ToolItem, ToolStatus};

/// Nombre de la herramienta de la lista de tareas. Su vista es `todo_lines`, no
/// una pieza de `atom_for`.
const TODO_WRITE: &str = "todo_write";

/// Estado en el que se muestra una llamada de ejemplo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixtureState {
    Pending,
    Running,
    Success,
    Error,
    AwaitingPermission,
    Canceled,
}

impl FixtureState {
    /// Todos los estados, en el orden en que los recorre el catálogo.
    pub const ALL: [FixtureState; 6] = [
        FixtureState::Pending,
        FixtureState::Running,
        FixtureState::Success,
        FixtureState::Error,
        FixtureState::AwaitingPermission,
        FixtureState::Canceled,
    ];

    /// Etiqueta visible en el catálogo.
    pub fn label(self) -> &'static str {
        match self {
            FixtureState::Pending => "pendiente",
            FixtureState::Running => "en curso",
            FixtureState::Success => "éxito",
            FixtureState::Error => "error",
            FixtureState::AwaitingPermission => "esperando permiso",
            FixtureState::Canceled => "cancelado",
        }
    }

    /// Estado siguiente en el ciclo del catálogo.
    pub fn next(self) -> FixtureState {
        let index = FixtureState::ALL
            .iter()
            .position(|state| *state == self)
            .unwrap_or(0);
        FixtureState::ALL[(index + 1) % FixtureState::ALL.len()]
    }
}

/// Llamada de ejemplo de una herramienta real.
#[derive(Debug, Clone)]
pub struct ToolFixture {
    /// Nombre de la herramienta tal como la envía el núcleo.
    pub name: &'static str,
    /// Entrada JSON cruda de la llamada (resumen de la pieza genérica).
    pub input: &'static str,
    /// Hechos que el adaptador extrae de `input`.
    pub facts: ToolFacts,
    /// Salida al terminar con éxito.
    pub output: &'static str,
    /// Salida al terminar con error.
    pub error: &'static str,
    /// Elementos de la lista; solo los usa `todo_write`.
    pub todos: &'static [&'static str],
}

impl ToolFixture {
    /// Llamada con el estado y el plegado dados. Los hechos ya van fijados, como
    /// hace la app al sincronizar la herramienta.
    pub fn item(&self, state: FixtureState, expanded: bool) -> ToolItem {
        let mut item = ToolItem::new(self.name, self.name);
        item.set_summary(self.input);
        item.facts = self.facts.clone();
        match state {
            FixtureState::Pending => {}
            FixtureState::Running => item.start(),
            FixtureState::Success => {
                item.set_result(false);
                item.set_output(self.output);
            }
            FixtureState::Error => {
                item.set_result(true);
                item.set_output(self.error);
            }
            FixtureState::AwaitingPermission => {
                item.start();
                item.await_permission();
            }
            FixtureState::Canceled => {
                item.start();
                item.cancel();
            }
        }
        if expanded {
            item.toggle_expanded();
        }
        item
    }

    /// Elementos de `todo_write` con el estado que les corresponde en `state`:
    /// los anteriores al actual están hechos, el actual sigue el estado de la
    /// llamada y los siguientes están pendientes. Vacío en el resto de herramientas.
    pub fn todo_items(&self, state: FixtureState) -> Vec<(&'static str, ToolStatus)> {
        let count = self.todos.len();
        let (done, current) = match state {
            FixtureState::Pending | FixtureState::AwaitingPermission => (0, ToolStatus::Pending),
            FixtureState::Running => (1, ToolStatus::Running),
            FixtureState::Success => (count, ToolStatus::Pending),
            FixtureState::Error => (1, ToolStatus::Error),
            FixtureState::Canceled => (1, ToolStatus::Canceled),
        };
        self.todos
            .iter()
            .enumerate()
            .map(|(index, text)| {
                let status = if index < done {
                    ToolStatus::Success
                } else if index == done {
                    current
                } else {
                    ToolStatus::Pending
                };
                (*text, status)
            })
            .collect()
    }

    /// Líneas que pinta el catálogo: la pieza de la herramienta (`tool_lines_with`)
    /// o, en `todo_write`, la lista de tareas (`todo_lines`).
    pub fn lines(&self, state: FixtureState, expanded: bool) -> Vec<Line> {
        if self.name == TODO_WRITE {
            return todo_lines(&self.todo_items(state));
        }
        let item = self.item(state, expanded);
        tool_lines_with(&item, &item.facts)
    }
}

/// Una llamada de ejemplo por cada herramienta real de CodeLoom.
pub fn fixtures() -> Vec<ToolFixture> {
    vec![
        ToolFixture {
            name: "bash",
            input: r#"{"command":"cargo test -p codeloom-core --all-features"}"#,
            facts: ToolFacts {
                primary: Some("cargo test -p codeloom-core --all-features".to_string()),
                ..ToolFacts::default()
            },
            output: "   Compiling codeloom-core v0.4.0\n    Finished `test` profile in 3.12s\n     Running unittests src/lib.rs\n\nrunning 18 tests\ntest session::tests::decodes_legacy ... ok\ntest session::tests::decodes_current ... ok\ntest session::tests::rejects_truncated ... ok\ntest tools::bash::tests::truncates_output ... ok\ntest tools::edit::tests::applies_once ... ok\ntest tools::grep::tests::counts_matches ... ok\ntest tools::view::tests::numbers_lines ... ok\ntest result: ok. 18 passed; 0 failed",
            error: "running 18 tests\ntest session::tests::decodes_legacy ... FAILED\n\nfailures:\n    session::tests::decodes_legacy\n\nthread 'session::tests::decodes_legacy' panicked at src/session.rs:57:9\nassertion `left == right` failed\n\ntest result: FAILED. 17 passed; 1 failed",
            todos: &[],
        },
        ToolFixture {
            name: "view",
            input: r#"{"path":"crates/codeloom-core/src/session.rs","offset":40,"limit":20}"#,
            facts: ToolFacts {
                primary: Some("crates/codeloom-core/src/session.rs".to_string()),
                range: Some((41, 60)),
                ..ToolFacts::default()
            },
            output: "41  pub fn load_session(data: &[u8]) -> Result<Session, LoadError> {\n42      decode_legacy_or_current(data)\n43  }\n44  \n45  fn decode_legacy_or_current(data: &[u8]) -> Result<Session, LoadError> {\n46      match data.first() {\n47          Some(b'{') => decode_json(data),\n48          _ => decode_binary(data),\n49      }\n50  }",
            error: "no such file: crates/codeloom-core/src/sesion.rs",
            todos: &[],
        },
        ToolFixture {
            name: "edit",
            input: r#"{"path":"crates/codeloom-core/src/session.rs","old":"pub fn load_session(data: &[u8]) -> Session {\n    decode(data)\n}\n","new":"pub fn load_session(data: &[u8]) -> Result<Session, LoadError> {\n    decode_legacy_or_current(data)\n}\n"}"#,
            facts: ToolFacts {
                primary: Some("crates/codeloom-core/src/session.rs".to_string()),
                before: Some(
                    "pub fn load_session(data: &[u8]) -> Session {\n    decode(data)\n}\n"
                        .to_string(),
                ),
                after: Some(
                    "pub fn load_session(data: &[u8]) -> Result<Session, LoadError> {\n    decode_legacy_or_current(data)\n}\n"
                        .to_string(),
                ),
                ..ToolFacts::default()
            },
            output: "edit applied",
            error: "old text not found in crates/codeloom-core/src/session.rs",
            todos: &[],
        },
        ToolFixture {
            name: "grep",
            input: r#"{"pattern":"load_session","glob":"*.rs"}"#,
            facts: ToolFacts {
                primary: Some("load_session".to_string()),
                secondary: Some("*.rs".to_string()),
                ..ToolFacts::default()
            },
            output: "crates/codeloom-core/src/session.rs:41:pub fn load_session(data: &[u8]) -> Result<Session, LoadError> {\ncrates/codeloom-core/src/session.rs:88:    load_session(&bytes)?\ncrates/codeloom-tui/src/app.rs:312:        let session = load_session(&path)?;\ncrates/codeloom-tui/src/app.rs:1480:            load_session(&legacy)\ntests/session_compat.rs:17:use codeloom_core::session::load_session;",
            error: "invalid regex: unclosed group",
            todos: &[],
        },
        ToolFixture {
            name: "glob",
            input: r#"{"pattern":"crates/**/*.rs"}"#,
            facts: ToolFacts {
                primary: Some("crates/**/*.rs".to_string()),
                ..ToolFacts::default()
            },
            output: "crates/codeloom-core/src/lib.rs\ncrates/codeloom-core/src/session.rs\ncrates/codeloom-core/src/tools/bash.rs\ncrates/codeloom-tui/src/app.rs\ncrates/codeloom-tui/src/tool_facts.rs",
            error: "permission denied: crates/private",
            todos: &[],
        },
        ToolFixture {
            name: "todo_write",
            input: r#"{"todos":["Inspeccionar la decodificación de sesiones","Añadir test de compatibilidad hacia atrás","Ejecutar la suite completa"]}"#,
            facts: ToolFacts::default(),
            output: "",
            error: "",
            todos: &[
                "Inspeccionar la decodificación de sesiones",
                "Añadir test de compatibilidad hacia atrás",
                "Ejecutar la suite completa",
            ],
        },
        ToolFixture {
            name: "ask_user",
            input: r#"{"questions":[{"question":"¿Qué formato de sesión usamos para la migración?","options":["JSON v2","Binario legado","Ambos"]}]}"#,
            facts: ToolFacts {
                primary: Some("¿Qué formato de sesión usamos para la migración?".to_string()),
                count: Some(3),
                ..ToolFacts::default()
            },
            output: "respuesta: Ambos",
            error: "no se pudo mostrar la pregunta",
            todos: &[],
        },
        ToolFixture {
            name: "subagent",
            input: r#"{"prompt":"Revisa la compatibilidad del decodificador con los fixtures legacy y dime qué falla","label":"inspect-compatibility"}"#,
            facts: ToolFacts {
                primary: Some("inspect-compatibility".to_string()),
                ..ToolFacts::default()
            },
            output: "Revisado el decodificador y los fixtures legacy.\nSin regresiones; 2 archivos revisados.",
            error: "el subagente agotó el límite de turnos",
            todos: &[],
        },
        ToolFixture {
            name: "subagent_control",
            input: r#"{"child_id":2,"action":"interrupt"}"#,
            facts: ToolFacts {
                primary: Some("interrupt".to_string()),
                secondary: Some("2".to_string()),
                ..ToolFacts::default()
            },
            output: "subagente 2 interrumpido",
            error: "subagente 2 no encontrado",
            todos: &[],
        },
        ToolFixture {
            name: "subagent_list",
            input: "{}",
            facts: ToolFacts::default(),
            output: "1  running  inspect-compatibility\n2  done     revisar tests de sesión\n3  pending  documentar el formato de sesión",
            error: "no se pudo leer la lista de subagentes",
            todos: &[],
        },
        ToolFixture {
            name: "job_start",
            input: r#"{"command":"cargo watch -x 'test -p codeloom-core'","label":"watch-core"}"#,
            facts: ToolFacts {
                primary: Some("cargo watch -x 'test -p codeloom-core'".to_string()),
                ..ToolFacts::default()
            },
            output: "job job-7 iniciado en segundo plano",
            error: "no se pudo lanzar el job: cargo-watch no está instalado",
            todos: &[],
        },
        ToolFixture {
            name: "job_list",
            input: "{}",
            facts: ToolFacts::default(),
            output: "job-7  running  cargo watch -x 'test -p codeloom-core'\njob-8  done     cargo clippy -- -D warnings\njob-9  done     cargo fmt --all --check",
            error: "no se pudo leer la lista de jobs",
            todos: &[],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{atom_for, tool_lines, Transcript, Viewport};
    use crate::widgets::transcript::{render, Layout};
    use crate::widgets::{Theme, ThemePreset};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    /// Herramientas reales de CodeLoom que deben tener fixture.
    const REAL_TOOLS: [&str; 12] = [
        "bash",
        "view",
        "edit",
        "grep",
        "glob",
        "todo_write",
        "ask_user",
        "subagent",
        "subagent_control",
        "subagent_list",
        "job_start",
        "job_list",
    ];

    /// Pinta `lines` en un terminal de 80x20 con la función de render de la app.
    fn draw(lines: Vec<Line>, theme: &Theme) {
        let mut transcript = Transcript::new();
        transcript.push_lines(lines);
        let mut viewport = Viewport::new();
        let mut layout = Layout::new();
        let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render(
                    frame,
                    area,
                    &transcript,
                    &mut viewport,
                    &mut layout,
                    None,
                    theme,
                );
            })
            .unwrap();
    }

    #[test]
    fn every_real_tool_has_a_fixture() {
        let fixtures = fixtures();
        for tool in REAL_TOOLS {
            assert!(
                fixtures.iter().any(|fixture| fixture.name == tool),
                "falta la fixture de {tool}"
            );
        }
        assert_eq!(fixtures.len(), REAL_TOOLS.len());
    }

    #[test]
    fn each_fixture_renders_in_every_state_without_panic() {
        for preset in ThemePreset::ALL {
            let theme = Theme::preset(preset);
            for fixture in fixtures() {
                for state in FixtureState::ALL {
                    for expanded in [false, true] {
                        draw(fixture.lines(state, expanded), &theme);
                    }
                }
            }
        }
    }

    #[test]
    fn catalog_uses_atom_for() {
        for fixture in fixtures() {
            for state in FixtureState::ALL {
                for expanded in [false, true] {
                    let lines = fixture.lines(state, expanded);
                    if fixture.name == TODO_WRITE {
                        assert_eq!(lines, todo_lines(&fixture.todo_items(state)));
                        continue;
                    }
                    let item = fixture.item(state, expanded);
                    assert_eq!(
                        lines,
                        tool_lines_with(&item, &item.facts),
                        "{} en {:?}",
                        fixture.name,
                        state
                    );
                    assert_eq!(lines, atom_for(fixture.name).lines(&item, &item.facts));
                }
            }
        }
    }

    #[test]
    fn fixtures_have_a_visible_summary_for_non_pending() {
        for fixture in fixtures() {
            if fixture.name == TODO_WRITE {
                continue;
            }
            for state in FixtureState::ALL {
                if state == FixtureState::Pending {
                    continue;
                }
                let item = fixture.item(state, false);
                let header = tool_lines_with(&item, &item.facts)[0].plain().to_string();
                match item.facts.primary.as_deref() {
                    // Con dato principal, la cabecera lo muestra y ya no es la genérica.
                    Some(primary) => {
                        assert!(
                            header.contains(primary),
                            "{} en {:?}: «{header}» sin «{primary}»",
                            fixture.name,
                            state
                        );
                        assert_ne!(header, tool_lines(&item)[0].plain());
                    }
                    // Los listados no tienen dato principal: su recuento sale de la
                    // salida, así que solo se exige con éxito (con salida).
                    None if state == FixtureState::Success => {
                        assert!(
                            header.ends_with(" items"),
                            "{} sin recuento: «{header}»",
                            fixture.name
                        );
                    }
                    None => {}
                }
            }
        }
    }

    #[test]
    fn each_fixture_has_lines_for_a_success_call() {
        // Comprobación barata de que la pieza produce líneas en el caso normal.
        for fixture in fixtures() {
            assert!(!fixture.lines(FixtureState::Success, false).is_empty());
        }
    }
}
