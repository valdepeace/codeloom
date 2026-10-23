//! Nombres del cable de A2A v1.0 y su traducción a los tipos del núcleo.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use codeloom_core::a2a::{Artifact, Message, Part, PartContent, Role};
use codeloom_core::task::TaskState;
use serde::{Deserialize, Serialize};

/// Fallo al leer un valor del cable.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WireError {
    #[error("unknown task state `{0}`")]
    UnknownState(String),
    #[error("invalid timestamp `{0}`")]
    InvalidTimestamp(String),
    #[error("a part must have exactly one of text, raw, url or data, and has {0}")]
    PartContent(usize),
    #[error("`raw` is not valid base64: {0}")]
    InvalidBase64(String),
}

/// Nombre del cable de un estado: el del proto v1.0, p. ej. `TASK_STATE_WORKING`.
pub fn state_to_wire(state: TaskState) -> &'static str {
    state.proto_name()
}

/// Estado a partir de su nombre del cable. `TASK_STATE_UNSPECIFIED` y los nombres de la v0.3
/// (`working`) no valen.
pub fn state_from_wire(name: &str) -> Result<TaskState, WireError> {
    TaskState::from_proto_name(name).ok_or_else(|| WireError::UnknownState(name.to_string()))
}

/// Rol en el cable: `ROLE_USER` o `ROLE_AGENT`. `ROLE_UNSPECIFIED` no se admite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireRole {
    #[serde(rename = "ROLE_USER")]
    User,
    #[serde(rename = "ROLE_AGENT")]
    Agent,
}

impl From<Role> for WireRole {
    fn from(role: Role) -> Self {
        match role {
            Role::User => WireRole::User,
            Role::Agent => WireRole::Agent,
        }
    }
}

impl From<WireRole> for Role {
    fn from(role: WireRole) -> Self {
        match role {
            WireRole::User => Role::User,
            WireRole::Agent => Role::Agent,
        }
    }
}

/// Días desde 1970-01-01 hasta una fecha civil (algoritmo de Howard Hinnant).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Fecha civil a partir de los días desde 1970-01-01.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// Marca de tiempo del cable (`YYYY-MM-DDTHH:mm:ss.sssZ`, UTC) a partir de milisegundos desde
/// la época.
pub fn timestamp_to_wire(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let millis = ms.rem_euclid(1000);
    let (year, month, day) = civil_from_days(secs.div_euclid(86_400));
    let rest = secs.rem_euclid(86_400);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}

/// Milisegundos desde la época a partir de una marca del cable. Exige UTC con `Z` (sin
/// desplazamientos), admite de 0 a 9 decimales y rechaza fechas anteriores a 1970.
pub fn timestamp_from_wire(text: &str) -> Result<i64, WireError> {
    let malo = || WireError::InvalidTimestamp(text.to_string());
    let cuerpo = text.strip_suffix('Z').ok_or_else(malo)?;
    let (fecha_hora, fraccion) = match cuerpo.split_once('.') {
        Some((a, f)) => (a, Some(f)),
        None => (cuerpo, None),
    };
    let (fecha, hora) = fecha_hora.split_once('T').ok_or_else(malo)?;
    let numero = |s: &str, ancho: usize| -> Result<i64, WireError> {
        if s.len() == ancho && s.bytes().all(|b| b.is_ascii_digit()) {
            s.parse().map_err(|_| malo())
        } else {
            Err(malo())
        }
    };
    let f: Vec<&str> = fecha.split('-').collect();
    let h: Vec<&str> = hora.split(':').collect();
    if f.len() != 3 || h.len() != 3 {
        return Err(malo());
    }
    let (year, month, day) = (numero(f[0], 4)?, numero(f[1], 2)?, numero(f[2], 2)?);
    let (hh, mm, ss) = (numero(h[0], 2)?, numero(h[1], 2)?, numero(h[2], 2)?);
    if year < 1970 || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(malo());
    }
    if hh > 23 || mm > 59 || ss > 59 {
        return Err(malo());
    }
    let dias = days_from_civil(year, month, day);
    // Un día inexistente (30 de febrero) no sobrevive a la ida y vuelta.
    if civil_from_days(dias) != (year, month, day) {
        return Err(malo());
    }
    let millis = match fraccion {
        None => 0,
        Some(f) if (1..=9).contains(&f.len()) && f.bytes().all(|b| b.is_ascii_digit()) => {
            let tres = format!("{:0<3}", &f[..f.len().min(3)]);
            tres.parse::<i64>().map_err(|_| malo())?
        }
        Some(_) => return Err(malo()),
    };
    Ok(((dias * 24 + hh) * 60 + mm) * 60_000 + ss * 1_000 + millis)
}

/// `Part` en el cable: lleva exactamente una de las claves `text`, `raw`, `url` o `data`
/// (sin clave `kind`), más `filename`, `mediaType` y `metadata` si existen. `raw` va en base64
/// estándar: decisión nuestra, la especificación leída no lo dice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WirePart {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

impl From<&Part> for WirePart {
    fn from(part: &Part) -> Self {
        let mut wire = WirePart {
            text: None,
            raw: None,
            url: None,
            data: None,
            filename: part.filename.clone(),
            media_type: part.media_type.clone(),
            metadata: part.metadata.clone(),
        };
        match &part.content {
            PartContent::Text(texto) => wire.text = Some(texto.clone()),
            PartContent::Raw(bytes) => wire.raw = Some(STANDARD.encode(bytes)),
            PartContent::Url(url) => wire.url = Some(url.clone()),
            PartContent::Data(valor) => wire.data = Some(valor.clone()),
        }
        wire
    }
}

impl TryFrom<WirePart> for Part {
    type Error = WireError;

    fn try_from(wire: WirePart) -> Result<Self, WireError> {
        let presentes = [
            wire.text.is_some(),
            wire.raw.is_some(),
            wire.url.is_some(),
            wire.data.is_some(),
        ]
        .into_iter()
        .filter(|presente| *presente)
        .count();
        if presentes != 1 {
            return Err(WireError::PartContent(presentes));
        }
        let content = if let Some(texto) = wire.text {
            PartContent::Text(texto)
        } else if let Some(raw) = wire.raw {
            PartContent::Raw(
                STANDARD
                    .decode(raw)
                    .map_err(|error| WireError::InvalidBase64(error.to_string()))?,
            )
        } else if let Some(url) = wire.url {
            PartContent::Url(url)
        } else {
            PartContent::Data(wire.data.unwrap_or(serde_json::Value::Null))
        };
        Ok(Part {
            content,
            metadata: wire.metadata,
            filename: wire.filename,
            media_type: wire.media_type,
        })
    }
}

fn partes_a_wire(parts: &[Part]) -> Vec<WirePart> {
    parts.iter().map(WirePart::from).collect()
}

fn partes_desde_wire(parts: Vec<WirePart>) -> Result<Vec<Part>, WireError> {
    parts.into_iter().map(Part::try_from).collect()
}

/// `Message` en el cable. Los opcionales ausentes y las listas vacías no se emiten.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireMessage {
    pub message_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub role: WireRole,
    pub parts: Vec<WirePart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reference_task_ids: Vec<String>,
}

impl From<&Message> for WireMessage {
    fn from(message: &Message) -> Self {
        WireMessage {
            message_id: message.message_id.clone(),
            context_id: message.context_id.clone(),
            task_id: message.task_id.clone(),
            role: message.role.into(),
            parts: partes_a_wire(&message.parts),
            metadata: message.metadata.clone(),
            extensions: message.extensions.clone(),
            reference_task_ids: message.reference_task_ids.clone(),
        }
    }
}

impl TryFrom<WireMessage> for Message {
    type Error = WireError;

    fn try_from(wire: WireMessage) -> Result<Self, WireError> {
        Ok(Message {
            message_id: wire.message_id,
            context_id: wire.context_id,
            task_id: wire.task_id,
            role: wire.role.into(),
            parts: partes_desde_wire(wire.parts)?,
            metadata: wire.metadata,
            extensions: wire.extensions,
            reference_task_ids: wire.reference_task_ids,
        })
    }
}

/// `Artifact` en el cable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireArtifact {
    pub artifact_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parts: Vec<WirePart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
}

impl From<&Artifact> for WireArtifact {
    fn from(artifact: &Artifact) -> Self {
        WireArtifact {
            artifact_id: artifact.artifact_id.clone(),
            name: artifact.name.clone(),
            description: artifact.description.clone(),
            parts: partes_a_wire(&artifact.parts),
            metadata: artifact.metadata.clone(),
            extensions: artifact.extensions.clone(),
        }
    }
}

impl TryFrom<WireArtifact> for Artifact {
    type Error = WireError;

    fn try_from(wire: WireArtifact) -> Result<Self, WireError> {
        Ok(Artifact {
            artifact_id: wire.artifact_id,
            name: wire.name,
            description: wire.description,
            parts: partes_desde_wire(wire.parts)?,
            metadata: wire.metadata,
            extensions: wire.extensions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_roundtrip_through_wire_names() {
        for state in TaskState::ALL {
            let nombre = state_to_wire(state);
            assert!(nombre.starts_with("TASK_STATE_"), "{nombre}");
            assert_eq!(state_from_wire(nombre), Ok(state));
        }
    }

    #[test]
    fn unspecified_and_v03_states_are_rejected() {
        for nombre in ["TASK_STATE_UNSPECIFIED", "working", "input-required", ""] {
            assert_eq!(
                state_from_wire(nombre),
                Err(WireError::UnknownState(nombre.to_string()))
            );
        }
    }

    #[test]
    fn role_serializes_as_proto_name() {
        assert_eq!(
            serde_json::to_string(&WireRole::from(Role::User)).unwrap(),
            "\"ROLE_USER\""
        );
        assert_eq!(
            serde_json::to_string(&WireRole::from(Role::Agent)).unwrap(),
            "\"ROLE_AGENT\""
        );
        assert_eq!(
            Role::from(serde_json::from_str::<WireRole>("\"ROLE_AGENT\"").unwrap()),
            Role::Agent
        );
        assert!(serde_json::from_str::<WireRole>("\"ROLE_UNSPECIFIED\"").is_err());
        assert!(serde_json::from_str::<WireRole>("\"user\"").is_err());
    }

    #[test]
    fn timestamp_has_millisecond_precision_and_z() {
        assert_eq!(timestamp_to_wire(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(timestamp_to_wire(1), "1970-01-01T00:00:00.001Z");
        // 2024-02-29 (bisiesto) 12:34:56.789 UTC.
        assert_eq!(
            timestamp_to_wire(1_709_210_096_789),
            "2024-02-29T12:34:56.789Z"
        );
    }

    #[test]
    fn timestamps_roundtrip() {
        for ms in [0, 1, 999, 86_400_000, 1_709_210_096_789, 1_792_000_000_123] {
            assert_eq!(timestamp_from_wire(&timestamp_to_wire(ms)), Ok(ms), "{ms}");
        }
        // Ejemplos de la especificación: sin decimales y con seis.
        assert_eq!(
            timestamp_from_wire("2024-03-15T10:15:00Z"),
            Ok(1_710_497_700_000)
        );
        assert_eq!(
            timestamp_from_wire("2025-04-17T17:47:09.680794Z"),
            Ok(1_744_912_029_680)
        );
    }

    #[test]
    fn timestamp_before_1970_or_with_offset_is_rejected() {
        for texto in [
            "1969-12-31T23:59:59.000Z",
            "2024-03-15T10:15:00+01:00",
            "2024-03-15T10:15:00",
            "2024-02-30T10:15:00Z",
            "2024-03-15T25:15:00Z",
            "2024-03-15 10:15:00Z",
            "ayer",
        ] {
            assert_eq!(
                timestamp_from_wire(texto),
                Err(WireError::InvalidTimestamp(texto.to_string())),
                "{texto}"
            );
        }
    }

    fn mensaje_completo() -> Message {
        Message {
            message_id: "m-1".into(),
            context_id: Some("7".into()),
            task_id: Some("9".into()),
            role: Role::Agent,
            parts: vec![
                Part::text("hola"),
                Part {
                    content: PartContent::Raw(vec![0, 1, 2, 255]),
                    metadata: Some(serde_json::json!({"k": 1})),
                    filename: Some("a.bin".into()),
                    media_type: Some("application/octet-stream".into()),
                },
                Part {
                    content: PartContent::Url("https://e.example/x.png".into()),
                    metadata: None,
                    filename: None,
                    media_type: Some("image/png".into()),
                },
                Part {
                    content: PartContent::Data(serde_json::json!({"a": [1, 2]})),
                    metadata: None,
                    filename: None,
                    media_type: Some("application/json".into()),
                },
            ],
            metadata: Some(serde_json::json!({"m": true})),
            extensions: vec!["urn:x".into()],
            reference_task_ids: vec!["3".into()],
        }
    }

    #[test]
    fn part_text_serializes_with_only_the_text_key() {
        let json = serde_json::to_value(WirePart::from(&Part::text("hola"))).unwrap();
        assert_eq!(json, serde_json::json!({"text": "hola"}));
    }

    #[test]
    fn part_raw_is_base64() {
        let parte = Part {
            content: PartContent::Raw(b"hola".to_vec()),
            metadata: None,
            filename: Some("a.txt".into()),
            media_type: Some("text/plain".into()),
        };
        let json = serde_json::to_value(WirePart::from(&parte)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"raw": "aG9sYQ==", "filename": "a.txt", "mediaType": "text/plain"})
        );
    }

    #[test]
    fn part_with_two_content_keys_is_rejected() {
        let wire: WirePart = serde_json::from_str(r#"{"text":"a","url":"https://x"}"#).unwrap();
        assert_eq!(Part::try_from(wire), Err(WireError::PartContent(2)));
    }

    #[test]
    fn part_without_content_is_rejected() {
        let wire: WirePart = serde_json::from_str(r#"{"filename":"a"}"#).unwrap();
        assert_eq!(Part::try_from(wire), Err(WireError::PartContent(0)));
    }

    #[test]
    fn invalid_base64_raw_is_rejected() {
        let wire: WirePart = serde_json::from_str(r#"{"raw":"esto no es base64!!"}"#).unwrap();
        assert!(matches!(
            Part::try_from(wire),
            Err(WireError::InvalidBase64(_))
        ));
    }

    #[test]
    fn message_uses_camel_case_and_omits_empty_fields() {
        let mensaje = Message {
            message_id: "m-1".into(),
            context_id: None,
            task_id: None,
            role: Role::User,
            parts: vec![Part::text("hola")],
            metadata: None,
            extensions: Vec::new(),
            reference_task_ids: Vec::new(),
        };
        let json = serde_json::to_value(WireMessage::from(&mensaje)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "messageId": "m-1",
                "role": "ROLE_USER",
                "parts": [{"text": "hola"}]
            })
        );
        let completo = serde_json::to_value(WireMessage::from(&mensaje_completo())).unwrap();
        assert_eq!(completo["contextId"], "7");
        assert_eq!(completo["referenceTaskIds"], serde_json::json!(["3"]));
        assert_eq!(completo["extensions"], serde_json::json!(["urn:x"]));
    }

    #[test]
    fn message_roundtrips() {
        let original = mensaje_completo();
        let texto = serde_json::to_string(&WireMessage::from(&original)).unwrap();
        let vuelta: WireMessage = serde_json::from_str(&texto).unwrap();
        assert_eq!(Message::try_from(vuelta), Ok(original));
    }

    #[test]
    fn artifact_roundtrips() {
        let original = Artifact {
            artifact_id: "a-1".into(),
            name: Some("informe".into()),
            description: Some("resultado".into()),
            parts: vec![Part::text("listo"), Part::text("hecho")],
            metadata: Some(serde_json::json!({"n": 2})),
            extensions: vec!["urn:y".into()],
        };
        let json = serde_json::to_value(WireArtifact::from(&original)).unwrap();
        assert_eq!(json["artifactId"], "a-1");
        let vuelta: WireArtifact = serde_json::from_value(json).unwrap();
        assert_eq!(Artifact::try_from(vuelta), Ok(original));
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let wire: WireMessage = serde_json::from_str(
            r#"{"messageId":"m","role":"ROLE_USER","parts":[{"text":"x","kind":"text"}],"nuevo":1}"#,
        )
        .unwrap();
        let mensaje = Message::try_from(wire).unwrap();
        assert_eq!(mensaje.text(), "x");
    }
}
