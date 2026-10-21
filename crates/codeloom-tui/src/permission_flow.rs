//! Traducción entre peticiones de permiso y preguntas del core y los diálogos de batten.

use batten::state::{Diff, PermissionBody, PermissionChoice, PermissionDialog, QuestionSpec};
use codeloom_core::permission::{Decision, PermissionRequest};
use codeloom_core::question::{Answer, Question};
use crossterm::event::KeyCode;
use tokio::sync::oneshot;

/// Canal por el que se devuelve la decisión al core.
pub type PermissionReply = oneshot::Sender<Decision>;

/// Canal por el que se devuelven las respuestas de `ask_user`; `None` si el usuario cancela.
pub type QuestionReply = oneshot::Sender<Option<Vec<Answer>>>;

/// Especificación de diálogo para una pregunta del core.
pub fn spec_for(question: &Question) -> QuestionSpec {
    QuestionSpec {
        header: question.header.clone(),
        question: question.question.clone(),
        options: question
            .options
            .iter()
            .map(|option| (option.label.clone(), option.description.clone()))
            .collect(),
        multi: question.multi_select,
    }
}

/// Diálogo para una petición. `detail` es el JSON del input de la tool.
// limitación: en `edit` los números de línea son relativos al fragmento
// `old`/`new`, no al fichero; leer el fichero si hace falta precisión.
pub fn dialog_for(req: &PermissionRequest) -> PermissionDialog {
    let parsed: Option<serde_json::Value> = serde_json::from_str(&req.detail).ok();
    let (subtitle, body) = match (req.tool.as_str(), parsed) {
        ("edit", Some(v)) => {
            let field = |k: &str| v[k].as_str().unwrap_or_default().to_string();
            let diff = Diff::new(&field("old"), &field("new"), 3);
            (field("path"), PermissionBody::Diff(diff))
        }
        ("bash", Some(v)) => {
            let cmd = v["command"].as_str().unwrap_or_default().to_string();
            (String::new(), PermissionBody::Text(cmd))
        }
        (_, Some(v)) => {
            let pretty = serde_json::to_string_pretty(&v).unwrap_or_else(|_| req.detail.clone());
            (String::new(), PermissionBody::Text(pretty))
        }
        (_, None) => (String::new(), PermissionBody::Text(req.detail.clone())),
    };
    PermissionDialog::new(req.tool.clone(), subtitle, body)
}

/// Aplica una tecla al diálogo. Devuelve la decisión si la tecla la cierra.
pub fn permission_key(d: &mut PermissionDialog, code: KeyCode) -> Option<Decision> {
    match code {
        KeyCode::Left | KeyCode::BackTab => {
            d.prev();
            None
        }
        KeyCode::Right | KeyCode::Tab => {
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
        KeyCode::Enter => Some(match d.choice() {
            PermissionChoice::Allow => Decision::Allow,
            PermissionChoice::AllowSession => Decision::AllowSession,
            PermissionChoice::Deny => Decision::Deny,
        }),
        KeyCode::Char('a') => Some(Decision::Allow),
        KeyCode::Char('s') => Some(Decision::AllowSession),
        KeyCode::Char('d') | KeyCode::Esc => Some(Decision::Deny),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(tool: &str, detail: &str) -> PermissionRequest {
        PermissionRequest {
            tool: tool.into(),
            action: "run".into(),
            detail: detail.into(),
        }
    }

    #[test]
    fn edit_becomes_a_diff_with_the_path() {
        let d = dialog_for(&req("edit", r#"{"path":"a.rs","old":"x\n","new":"y\n"}"#));
        assert_eq!(d.subtitle, "a.rs");
        match d.body {
            PermissionBody::Diff(diff) => assert_eq!((diff.added(), diff.removed()), (1, 1)),
            other => panic!("esperaba diff, llegó {other:?}"),
        }
    }

    #[test]
    fn bash_shows_the_command() {
        let d = dialog_for(&req("bash", r#"{"command":"ls -la"}"#));
        assert_eq!(d.body, PermissionBody::Text("ls -la".into()));
    }

    #[test]
    fn other_tools_show_pretty_json_and_invalid_json_raw() {
        let d = dialog_for(&req("view", r#"{"path":"a.rs"}"#));
        assert!(matches!(&d.body, PermissionBody::Text(t) if t.contains("\"path\": \"a.rs\"")));
        let d = dialog_for(&req("view", "no es json"));
        assert_eq!(d.body, PermissionBody::Text("no es json".into()));
    }

    #[test]
    fn keys_map_to_decisions() {
        let mut d = dialog_for(&req("bash", r#"{"command":"ls"}"#));
        assert_eq!(permission_key(&mut d, KeyCode::Right), None);
        assert_eq!(
            permission_key(&mut d, KeyCode::Enter),
            Some(Decision::AllowSession)
        );
        assert_eq!(
            permission_key(&mut d, KeyCode::Char('a')),
            Some(Decision::Allow)
        );
        assert_eq!(permission_key(&mut d, KeyCode::Esc), Some(Decision::Deny));
        assert_eq!(permission_key(&mut d, KeyCode::Char('x')), None);
    }
}
