//! Nombres del cable de A2A v1.0 y su traducción a los tipos del núcleo.

use codeloom_core::a2a::Role;
use codeloom_core::task::TaskState;
use serde::{Deserialize, Serialize};

/// Fallo al leer un valor del cable.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WireError {
    #[error("unknown task state `{0}`")]
    UnknownState(String),
    #[error("invalid timestamp `{0}`")]
    InvalidTimestamp(String),
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
}
