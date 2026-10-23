//! Perfil de un agente: el fichero `<nombre>.md`, con frontmatter YAML y cuerpo markdown.
//!
//! Formato de la sección 2 de `docs/specs/2026-10-15-loom-agent-profile-a2a.md`, con las
//! decisiones de `docs/specs/2026-10-22-codeloom-a2a-agent-registry-design.md`. Hoy se
//! interpretan `name`, `description`, `version`, `persona`, `model`, `effort`, `max_turns` y
//! `max_depth`. `transport: a2a-remote` con `a2a.endpoint` declara un agente remoto (A8a) y se
//! aplica. `scope`, `delegation` y cualquier otro `transport` o `a2a` se leen, se declaran como no
//! aplicados y no se ejecutan. Cualquier otra clave se ignora con aviso: un campo nuevo no
//! rompe la carga.

use serde_yaml_bw::{Mapping, Value};

/// Perfil ya validado de un agente local.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProfile {
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub persona_role: Option<String>,
    pub persona_goal: Option<String>,
    pub persona_backstory: Option<String>,
    /// Cuerpo markdown del fichero; es el prompt de sistema del agente.
    pub body: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub max_turns: Option<u32>,
    pub max_depth: Option<u32>,
    /// Claves presentes que se leen pero no se aplican todavía: `scope`, `delegation`,
    /// `transport` y `a2a`.
    pub declared_not_applied: Vec<String>,
    /// Punto de entrada del agente remoto (`transport: a2a-remote` y `a2a.endpoint`). Con él el
    /// perfil es un puntero a un remoto, no un agente local: persona, modelo y cuerpo no se usan.
    pub remote_endpoint: Option<String>,
}

/// Problema de un perfil. Los avisos viajan junto al perfil; los errores impiden cargarlo.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProfileProblem {
    #[error("nombre de agente no válido: `{0}`")]
    InvalidName(String),
    #[error("el `name` `{declared}` no coincide con el nombre del fichero `{file}`")]
    NameMismatch { file: String, declared: String },
    #[error("falta el campo obligatorio `{0}`")]
    MissingField(&'static str),
    #[error("frontmatter no válido: {0}")]
    BadFrontmatter(String),
    #[error("campo desconocido `{0}` (se ignora)")]
    UnknownField(String),
    #[error("campo `{0}` declarado y no aplicado todavía")]
    DeclaredNotApplied(String),
}

/// Lee un perfil. `file_stem` es el nombre del fichero sin extensión; el `name` del
/// frontmatter debe coincidir con él.
///
/// `Err` = el perfil no se puede cargar. Los avisos (campos desconocidos, campos declarados
/// y no aplicados) van en el `Vec`. El texto se normaliza a LF antes de leerlo.
pub fn parse_profile(
    file_stem: &str,
    text: &str,
) -> Result<(AgentProfile, Vec<ProfileProblem>), ProfileProblem> {
    if !is_valid_agent_name(file_stem) {
        return Err(ProfileProblem::InvalidName(file_stem.to_string()));
    }

    // Se quita el BOM, si lo hay, y se normalizan los finales de línea antes de partir.
    let normalizado = text
        .strip_prefix('\u{feff}')
        .unwrap_or(text)
        .replace("\r\n", "\n");
    let (frontmatter, cuerpo) = separar_frontmatter(&normalizado)?;

    let valor: Value = serde_yaml_bw::from_str(frontmatter)
        .map_err(|error| ProfileProblem::BadFrontmatter(error.to_string()))?;
    let Some(mapa) = valor.as_mapping() else {
        return Err(ProfileProblem::BadFrontmatter(
            "el frontmatter debe ser un mapa YAML".to_string(),
        ));
    };

    let remote_endpoint = endpoint_remoto(mapa)?;

    // Las claves desconocidas avisan; las declaradas-y-no-aplicadas avisan y se anotan.
    let mut avisos = Vec::new();
    let mut declared_not_applied = Vec::new();
    for (clave, _) in mapa.iter() {
        let Some(clave) = clave.as_str() else {
            return Err(ProfileProblem::BadFrontmatter(
                "clave no textual en el frontmatter".to_string(),
            ));
        };
        if CLAVES_CONOCIDAS.contains(&clave) {
            continue;
        }
        // Un perfil remoto aplica `transport` y `a2a`: ya no son «declarados sin aplicar».
        if remote_endpoint.is_some() && (clave == "transport" || clave == "a2a") {
            continue;
        }
        if CLAVES_NO_APLICADAS.contains(&clave) {
            declared_not_applied.push(clave.to_string());
            avisos.push(ProfileProblem::DeclaredNotApplied(clave.to_string()));
        } else {
            avisos.push(ProfileProblem::UnknownField(clave.to_string()));
        }
    }

    let nombre = texto_obligatorio(mapa, "name")?;
    if !is_valid_agent_name(&nombre) {
        return Err(ProfileProblem::InvalidName(nombre));
    }
    if nombre != file_stem {
        return Err(ProfileProblem::NameMismatch {
            file: file_stem.to_string(),
            declared: nombre,
        });
    }
    let description = texto_obligatorio(mapa, "description")?;

    let (persona_role, persona_goal, persona_backstory) = match mapa.get("persona") {
        None | Some(Value::Null(_)) => (None, None, None),
        Some(persona) => {
            let Some(persona) = persona.as_mapping() else {
                return Err(ProfileProblem::BadFrontmatter(
                    "`persona` debe ser un mapa".to_string(),
                ));
            };
            for (clave, _) in persona.iter() {
                match clave.as_str() {
                    Some("role" | "goal" | "backstory") => {}
                    Some(otra) => {
                        avisos.push(ProfileProblem::UnknownField(format!("persona.{otra}")))
                    }
                    None => {
                        return Err(ProfileProblem::BadFrontmatter(
                            "clave de `persona` no textual".to_string(),
                        ))
                    }
                }
            }
            (
                texto_opcional(persona, "role")?,
                texto_opcional(persona, "goal")?,
                texto_opcional(persona, "backstory")?,
            )
        }
    };

    let perfil = AgentProfile {
        name: nombre,
        description,
        version: texto_opcional(mapa, "version")?,
        persona_role,
        persona_goal,
        persona_backstory,
        body: cuerpo.trim().to_string(),
        model: texto_opcional(mapa, "model")?,
        effort: texto_opcional(mapa, "effort")?,
        max_turns: entero_opcional(mapa, "max_turns")?,
        max_depth: entero_opcional(mapa, "max_depth")?,
        declared_not_applied,
        remote_endpoint,
    };
    Ok((perfil, avisos))
}

/// Claves que la carga interpreta hoy.
const CLAVES_CONOCIDAS: &[&str] = &[
    "name",
    "description",
    "version",
    "persona",
    "model",
    "effort",
    "max_turns",
    "max_depth",
];

/// Claves que se leen y se declaran, pero que no se aplican todavía.
const CLAVES_NO_APLICADAS: &[&str] = &["scope", "delegation", "transport", "a2a"];

/// Valor de `transport` que declara un agente remoto.
const TRANSPORTE_REMOTO: &str = "a2a-remote";

/// `a2a.endpoint` de un perfil remoto. `None` si el perfil no es remoto; si lo es y falta el
/// endpoint o no es una URL admisible (HTTPS, o HTTP solo en bucle local), el perfil no carga.
fn endpoint_remoto(mapa: &Mapping) -> Result<Option<String>, ProfileProblem> {
    if texto_opcional(mapa, "transport")?.as_deref() != Some(TRANSPORTE_REMOTO) {
        return Ok(None);
    }
    let endpoint = match mapa.get("a2a") {
        Some(a2a) => match a2a.as_mapping() {
            Some(a2a) => texto_opcional(a2a, "endpoint")?,
            None => {
                return Err(ProfileProblem::BadFrontmatter(
                    "`a2a` debe ser un mapa".to_string(),
                ))
            }
        },
        None => None,
    };
    let Some(endpoint) = endpoint
        .map(|e| e.trim().to_string())
        .filter(|e| !e.is_empty())
    else {
        return Err(ProfileProblem::MissingField("a2a.endpoint"));
    };
    crate::config::validate_base_url(&endpoint).map_err(|motivo| {
        ProfileProblem::BadFrontmatter(format!("`a2a.endpoint` no es admisible: {motivo}"))
    })?;
    Ok(Some(endpoint))
}

/// Parte el texto (ya en LF) en frontmatter y cuerpo. El frontmatter va entre dos líneas
/// `---`; el cuerpo es todo lo que sigue a la segunda.
fn separar_frontmatter(texto: &str) -> Result<(&str, &str), ProfileProblem> {
    let fin_primera = texto.find('\n').unwrap_or(texto.len());
    if texto[..fin_primera].trim_end() != "---" {
        return Err(ProfileProblem::BadFrontmatter(
            "el fichero debe empezar por una línea `---`".to_string(),
        ));
    }

    let resto_inicio = (fin_primera + 1).min(texto.len());
    let resto = &texto[resto_inicio..];
    let mut inicio_linea = 0;
    for linea in resto.split_inclusive('\n') {
        if linea.trim_end() == "---" {
            let frontmatter = &resto[..inicio_linea];
            let cuerpo = &resto[inicio_linea + linea.len()..];
            return Ok((frontmatter, cuerpo));
        }
        inicio_linea += linea.len();
    }
    Err(ProfileProblem::BadFrontmatter(
        "el frontmatter no se cierra con una línea `---`".to_string(),
    ))
}

/// Texto de un campo: admite cadenas y números (p. ej. `version: 1`).
fn valor_texto(valor: &Value, clave: &str) -> Result<String, ProfileProblem> {
    match valor {
        Value::String(texto, _) => Ok(texto.clone()),
        Value::Number(numero, _) => Ok(numero.to_string()),
        _ => Err(ProfileProblem::BadFrontmatter(format!(
            "el campo `{clave}` debe ser texto"
        ))),
    }
}

/// Campo de texto opcional: ausente o `null` cuentan como no declarado.
fn texto_opcional(mapa: &Mapping, clave: &str) -> Result<Option<String>, ProfileProblem> {
    match mapa.get(clave) {
        None | Some(Value::Null(_)) => Ok(None),
        Some(valor) => valor_texto(valor, clave).map(Some),
    }
}

/// Campo de texto obligatorio: ausente, `null` o vacío dan `MissingField`.
fn texto_obligatorio(mapa: &Mapping, clave: &'static str) -> Result<String, ProfileProblem> {
    match texto_opcional(mapa, clave)? {
        Some(texto) if !texto.trim().is_empty() => Ok(texto),
        _ => Err(ProfileProblem::MissingField(clave)),
    }
}

/// Entero sin signo opcional, para `max_turns` y `max_depth`.
fn entero_opcional(mapa: &Mapping, clave: &str) -> Result<Option<u32>, ProfileProblem> {
    match mapa.get(clave) {
        None | Some(Value::Null(_)) => Ok(None),
        Some(valor) => valor
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .map(Some)
            .ok_or_else(|| {
                ProfileProblem::BadFrontmatter(format!("`{clave}` debe ser un entero no negativo"))
            }),
    }
}

/// `true` si `name` es un identificador de agente válido: solo `a-z`, `0-9` y `-`, no vacío,
/// sin `-` al principio ni al final, y de 64 caracteres como máximo.
pub fn is_valid_agent_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('-')
        && !name.ends_with('-')
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn perfil_completo() -> String {
        r#"---
name: frontend
description: Especialista en UI, componentes y accesibilidad
version: 1
persona:
  role: Ingeniero de frontend
  goal: Implementar y revisar UI del proyecto
  backstory: Prefiere componentes pequeños, testeables y accesibles
scope:
  mcp: [serena, figma]
  skills: [ponytail, accessibility]
  tools: [view, edit, glob, grep, bash]
model: claude-sonnet
effort: high
max_turns: 20
max_depth: 1
delegation:
  allow: true
  policy: both
transport: a2a-remote
a2a:
  endpoint: https://devops.internal/.well-known/agent-card.json
  card: devops
---

Eres el agente de frontend. Solo atiendes tareas de UI, componentes y accesibilidad.
Si la tarea es de otra especialidad, dilo y delega con `delegate`.
"#
        .to_string()
    }

    fn perfil_minimo() -> String {
        "---\nname: frontend\ndescription: Un agente de prueba\n---\n\nEres el agente.\n"
            .to_string()
    }

    fn texto_con_nombre(nombre: &str) -> String {
        format!("---\nname: '{nombre}'\ndescription: Un agente de prueba\n---\n\nCuerpo.\n")
    }

    #[test]
    fn parses_a_complete_profile() {
        let (perfil, problemas) = parse_profile("frontend", &perfil_completo()).unwrap();
        assert_eq!(perfil.name, "frontend");
        assert_eq!(
            perfil.description,
            "Especialista en UI, componentes y accesibilidad"
        );
        assert_eq!(perfil.version.as_deref(), Some("1"));
        assert_eq!(
            perfil.persona_role.as_deref(),
            Some("Ingeniero de frontend")
        );
        assert_eq!(
            perfil.persona_goal.as_deref(),
            Some("Implementar y revisar UI del proyecto")
        );
        assert_eq!(
            perfil.persona_backstory.as_deref(),
            Some("Prefiere componentes pequeños, testeables y accesibles")
        );
        assert_eq!(perfil.model.as_deref(), Some("claude-sonnet"));
        assert_eq!(perfil.effort.as_deref(), Some("high"));
        assert_eq!(perfil.max_turns, Some(20));
        assert_eq!(perfil.max_depth, Some(1));
        assert_eq!(
            perfil.body,
            "Eres el agente de frontend. Solo atiendes tareas de UI, componentes y accesibilidad.\nSi la tarea es de otra especialidad, dilo y delega con `delegate`."
        );
        // `transport: a2a-remote` con `a2a.endpoint` se aplica (A8a); `scope` y `delegation`, no.
        assert_eq!(
            perfil.remote_endpoint.as_deref(),
            Some("https://devops.internal/.well-known/agent-card.json")
        );
        assert_eq!(perfil.declared_not_applied.len(), 2);
        for clave in ["scope", "delegation"] {
            assert!(perfil.declared_not_applied.iter().any(|c| c == clave));
            assert!(problemas.contains(&ProfileProblem::DeclaredNotApplied(clave.to_string())));
        }
        for clave in ["transport", "a2a"] {
            assert!(!problemas.contains(&ProfileProblem::DeclaredNotApplied(clave.to_string())));
        }
        assert!(!problemas
            .iter()
            .any(|p| matches!(p, ProfileProblem::UnknownField(_))));
    }

    #[test]
    fn parses_a_minimal_profile() {
        let (perfil, problemas) = parse_profile("frontend", &perfil_minimo()).unwrap();
        assert_eq!(perfil.name, "frontend");
        assert_eq!(perfil.description, "Un agente de prueba");
        assert_eq!(perfil.version, None);
        assert_eq!(perfil.persona_role, None);
        assert_eq!(perfil.persona_goal, None);
        assert_eq!(perfil.persona_backstory, None);
        assert_eq!(perfil.model, None);
        assert_eq!(perfil.effort, None);
        assert_eq!(perfil.max_turns, None);
        assert_eq!(perfil.max_depth, None);
        assert!(perfil.declared_not_applied.is_empty());
        assert_eq!(perfil.body, "Eres el agente.");
        assert!(problemas.is_empty());
    }

    #[test]
    fn unknown_fields_warn_but_load() {
        let texto =
            "---\nname: frontend\ndescription: Un agente de prueba\ncolor: azul\n---\n\nCuerpo.\n";
        let (perfil, problemas) = parse_profile("frontend", texto).unwrap();
        assert_eq!(perfil.name, "frontend");
        assert!(problemas.contains(&ProfileProblem::UnknownField("color".to_string())));
    }

    #[test]
    fn declared_scope_is_reported_as_not_applied() {
        let texto = "---\nname: frontend\ndescription: Un agente de prueba\nscope:\n  tools: [view]\n---\n\nCuerpo.\n";
        let (perfil, problemas) = parse_profile("frontend", texto).unwrap();
        assert_eq!(perfil.declared_not_applied, vec!["scope".to_string()]);
        assert!(problemas.contains(&ProfileProblem::DeclaredNotApplied("scope".to_string())));
    }

    #[test]
    fn rejects_invalid_names() {
        for nombre in [
            "Frontend",
            "..",
            "a/b",
            "a\\b",
            "",
            "-x",
            "x-",
            "a_b",
            "frontend-ñ",
        ] {
            let resultado = parse_profile(nombre, &texto_con_nombre(nombre));
            assert!(
                matches!(resultado, Err(ProfileProblem::InvalidName(_))),
                "debía rechazar {nombre:?}"
            );
        }
        for valido in ["frontend", "a1", "x-y-2"] {
            assert!(is_valid_agent_name(valido), "debía aceptar {valido:?}");
        }
        assert!(is_valid_agent_name(&"a".repeat(64)));
        assert!(!is_valid_agent_name(&"a".repeat(65)));
    }

    #[test]
    fn rejects_name_that_differs_from_file_stem() {
        let resultado = parse_profile("frontend", &texto_con_nombre("backend"));
        assert_eq!(
            resultado,
            Err(ProfileProblem::NameMismatch {
                file: "frontend".to_string(),
                declared: "backend".to_string(),
            })
        );
    }

    #[test]
    fn rejects_missing_description() {
        let texto = "---\nname: frontend\n---\n\nCuerpo.\n";
        let resultado = parse_profile("frontend", texto);
        assert_eq!(
            resultado.unwrap_err(),
            ProfileProblem::MissingField("description")
        );
    }

    #[test]
    fn rejects_missing_or_unterminated_frontmatter() {
        let sin_delimitadores = "name: frontend\ndescription: x\n";
        let sin_cierre = "---\nname: frontend\ndescription: x\n";
        let vacio = "";
        for texto in [sin_delimitadores, sin_cierre, vacio] {
            let resultado = parse_profile("frontend", texto);
            assert!(
                matches!(resultado, Err(ProfileProblem::BadFrontmatter(_))),
                "debía rechazar {texto:?}"
            );
        }
    }

    #[test]
    fn body_is_everything_after_the_frontmatter() {
        let texto = "---\nname: frontend\ndescription: x\n---\n\nPrimera línea.\n\n---\nSegunda sección con regla.\n";
        let (perfil, _) = parse_profile("frontend", texto).unwrap();
        assert_eq!(
            perfil.body,
            "Primera línea.\n\n---\nSegunda sección con regla."
        );
    }

    #[test]
    fn crlf_input_is_accepted() {
        let lf = parse_profile("frontend", &perfil_completo()).unwrap().0;
        let crlf = parse_profile("frontend", &perfil_completo().replace('\n', "\r\n"))
            .unwrap()
            .0;
        assert_eq!(crlf, lf);
    }

    fn perfil_remoto(frontmatter_extra: &str) -> String {
        format!("---\nname: lejano\ndescription: Agente remoto\n{frontmatter_extra}---\n")
    }

    #[test]
    fn a_remote_profile_carries_its_endpoint_and_is_not_reported_as_not_applied() {
        let texto =
            perfil_remoto("transport: a2a-remote\na2a:\n  endpoint: https://agente.example\n");

        let (perfil, avisos) = parse_profile("lejano", &texto).unwrap();

        assert_eq!(
            perfil.remote_endpoint.as_deref(),
            Some("https://agente.example")
        );
        assert!(perfil.declared_not_applied.is_empty(), "{perfil:?}");
        assert!(avisos.is_empty(), "{avisos:?}");
    }

    #[test]
    fn a_remote_profile_without_an_endpoint_does_not_load() {
        for extra in [
            "transport: a2a-remote\n",
            "transport: a2a-remote\na2a:\n  endpoint: \"  \"\n",
            "transport: a2a-remote\na2a:\n  otra: 1\n",
        ] {
            let resultado = parse_profile("lejano", &perfil_remoto(extra));
            assert_eq!(
                resultado,
                Err(ProfileProblem::MissingField("a2a.endpoint")),
                "{extra}"
            );
        }
    }

    #[test]
    fn a_remote_endpoint_over_plain_http_outside_loopback_does_not_load() {
        let texto =
            perfil_remoto("transport: a2a-remote\na2a:\n  endpoint: http://agente.example\n");

        let resultado = parse_profile("lejano", &texto);

        assert!(
            matches!(resultado, Err(ProfileProblem::BadFrontmatter(_))),
            "{resultado:?}"
        );
    }

    #[test]
    fn another_transport_stays_declared_and_not_applied() {
        let texto = perfil_remoto("transport: in-process\n");

        let (perfil, _) = parse_profile("lejano", &texto).unwrap();

        assert_eq!(perfil.remote_endpoint, None);
        assert!(perfil
            .declared_not_applied
            .contains(&"transport".to_string()));
    }
}
