//! Tipos de A2A (`lf.a2a.v1`) que usa el núcleo: `Message`, `Part` y `Artifact`.
//!
//! Los campos y sus nombres son los del proto. La serialización de este módulo es un
//! formato interno de almacenamiento y no el del cable (ese es otro trabajo).

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

/// Rol del emisor de un mensaje. Se omite `ROLE_UNSPECIFIED` del proto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Agent,
}

/// Contenido de una `Part`: el `oneof` del proto con sus cuatro variantes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartContent {
    Text(String),
    Raw(Vec<u8>),
    Url(String),
    Data(serde_json::Value),
}

/// Fragmento de contenido de un `Message` o de un `Artifact`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Part {
    pub content: PartContent,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub media_type: Option<String>,
}

/// Mensaje de A2A: un turno de comunicación entre un cliente y un agente.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub message_id: String,
    #[serde(default)]
    pub context_id: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
    pub role: Role,
    pub parts: Vec<Part>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub reference_task_ids: Vec<String>,
}

/// Resultado producido por una tarea de A2A.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub artifact_id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub parts: Vec<Part>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub extensions: Vec<String>,
}

impl Part {
    /// Parte de texto plano, sin metadatos.
    pub fn text(text: impl Into<String>) -> Part {
        Part {
            content: PartContent::Text(text.into()),
            metadata: None,
            filename: None,
            media_type: None,
        }
    }

    /// Texto de la parte si es de tipo `Text`; `None` en cualquier otra variante.
    pub fn as_text(&self) -> Option<&str> {
        match &self.content {
            PartContent::Text(texto) => Some(texto.as_str()),
            _ => None,
        }
    }
}

/// Une con `"\n"` el texto de las `Part`s de texto; ignora el resto.
fn unir_texto(parts: &[Part]) -> String {
    parts
        .iter()
        .filter_map(Part::as_text)
        .collect::<Vec<_>>()
        .join("\n")
}

impl Message {
    /// Mensaje de estado emitido por el agente: `role = Agent`, una `Part` de texto y un
    /// `message_id` generado con `new_id`.
    pub fn agent_text(text: impl Into<String>) -> Message {
        Message {
            message_id: new_id(),
            context_id: None,
            task_id: None,
            role: Role::Agent,
            parts: vec![Part::text(text)],
            metadata: None,
            extensions: vec![],
            reference_task_ids: vec![],
        }
    }

    /// Une con `"\n"` el texto de las `Part`s de texto; ignora el resto. Cadena vacía si
    /// no hay ninguna.
    pub fn text(&self) -> String {
        unir_texto(&self.parts)
    }
}

impl Artifact {
    /// Artefacto de una sola `Part` de texto, con `artifact_id` generado con `new_id`.
    pub fn text_artifact(text: impl Into<String>) -> Artifact {
        Artifact {
            artifact_id: new_id(),
            name: None,
            description: None,
            parts: vec![Part::text(text)],
            metadata: None,
            extensions: vec![],
        }
    }

    /// Une con `"\n"` el texto de las `Part`s de texto; cadena vacía si no hay ninguna.
    pub fn text(&self) -> String {
        unir_texto(&self.parts)
    }
}

/// Une con `"\n"` el texto de cada artefacto.
pub fn artifacts_text(artifacts: &[Artifact]) -> String {
    artifacts
        .iter()
        .map(Artifact::text)
        .collect::<Vec<_>>()
        .join("\n")
}

static CONTADOR: AtomicU64 = AtomicU64::new(0);

/// Genera un identificador para `message_id` y `artifact_id`.
///
/// El núcleo no tenía generador de ids de cadena, así que se combina un contador atómico
/// del proceso con `message::now_ms`, que es el reloj ya existente en el núcleo.
pub fn new_id() -> String {
    let secuencia = CONTADOR.fetch_add(1, Ordering::Relaxed);
    format!("{:x}-{:x}", crate::message::now_ms(), secuencia)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashSet;

    #[test]
    fn part_roundtrips_through_serde_for_every_variant() {
        let variantes = vec![
            Part::text("hola"),
            Part {
                content: PartContent::Raw(vec![0, 127, 255]),
                metadata: Some(json!({"origen": "binario"})),
                filename: Some("datos.bin".into()),
                media_type: Some("application/octet-stream".into()),
            },
            Part {
                content: PartContent::Url("https://ejemplo.invalid/recurso".into()),
                metadata: None,
                filename: None,
                media_type: Some("text/plain".into()),
            },
            Part {
                content: PartContent::Data(json!({"ok": true, "n": [1, 2]})),
                metadata: None,
                filename: None,
                media_type: None,
            },
        ];
        for parte in variantes {
            let json = serde_json::to_string(&parte).unwrap();
            let vuelta: Part = serde_json::from_str(&json).unwrap();
            assert_eq!(vuelta, parte);
        }
    }

    #[test]
    fn message_text_joins_text_parts_and_ignores_the_rest() {
        let m = Message {
            message_id: "m1".into(),
            context_id: None,
            task_id: None,
            role: Role::Agent,
            parts: vec![
                Part::text("uno"),
                Part {
                    content: PartContent::Data(json!({"k": 1})),
                    metadata: None,
                    filename: None,
                    media_type: None,
                },
                Part::text("dos"),
                Part {
                    content: PartContent::Url("https://ejemplo.invalid".into()),
                    metadata: None,
                    filename: None,
                    media_type: None,
                },
            ],
            metadata: None,
            extensions: vec![],
            reference_task_ids: vec![],
        };
        assert_eq!(m.text(), "uno\ndos");
    }

    #[test]
    fn message_without_text_parts_has_empty_text() {
        let m = Message {
            message_id: "m2".into(),
            context_id: None,
            task_id: None,
            role: Role::User,
            parts: vec![Part {
                content: PartContent::Raw(vec![1, 2, 3]),
                metadata: None,
                filename: None,
                media_type: None,
            }],
            metadata: None,
            extensions: vec![],
            reference_task_ids: vec![],
        };
        assert_eq!(m.text(), "");
    }

    #[test]
    fn artifact_text_joins_text_parts() {
        let a = Artifact {
            artifact_id: "a1".into(),
            name: Some("resultado".into()),
            description: None,
            parts: vec![
                Part::text("primera"),
                Part {
                    content: PartContent::Data(json!(42)),
                    metadata: None,
                    filename: None,
                    media_type: None,
                },
                Part::text("segunda"),
            ],
            metadata: None,
            extensions: vec![],
        };
        assert_eq!(a.text(), "primera\nsegunda");
    }

    #[test]
    fn artifacts_text_joins_artifacts_with_newline() {
        let uno = Artifact::text_artifact("alfa");
        let dos = Artifact::text_artifact("beta");
        assert_eq!(artifacts_text(&[uno, dos]), "alfa\nbeta");
        assert_eq!(artifacts_text(&[]), "");
    }

    #[test]
    fn agent_text_uses_agent_role_and_one_text_part() {
        let m = Message::agent_text("listo");
        assert_eq!(m.role, Role::Agent);
        assert_eq!(m.parts.len(), 1);
        assert_eq!(m.parts[0].as_text(), Some("listo"));
        assert!(!m.message_id.is_empty());
        assert_eq!(m.text(), "listo");
    }

    #[test]
    fn new_id_is_unique_across_calls() {
        let ids: HashSet<String> = (0..1000).map(|_| new_id()).collect();
        assert_eq!(ids.len(), 1000);
    }
}
