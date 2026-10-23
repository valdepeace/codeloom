//! Tarjeta del agente tal como llega por el cable (v1.0) y su conversión a la del núcleo.
//!
//! Cabe señalar que una tarjeta remota es dato de un tercero: aquí se lee con tolerancia (se
//! exige solo lo que se usa) y se **acota cada campo** al convertirla, de modo que ni una tarjeta
//! enorme ni un texto desmedido llegan enteros al resto del programa. Lo que A6 no modela
//! (esquemas de seguridad, firmas, icono) se ignora, salvo el hecho de que el remoto pide
//! autenticación, que se anota porque este cliente todavía no sabe darla (A8d).

use codeloom_core::card::{
    AgentCapabilities, AgentCard, AgentExtension, AgentInterface, AgentProvider, AgentSkill,
};
use serde::Deserialize;

/// Binding que este cliente sabe hablar.
pub const BINDING_JSONRPC: &str = "JSONRPC";
/// Única versión del protocolo que este cliente implementa. Un `1.1` no consta como compatible.
pub const VERSION_SOPORTADA: &str = "1.0";

const MAX_NOMBRE: usize = 200;
const MAX_TEXTO: usize = 2_000;
const MAX_INTERFACES: usize = 10;
const MAX_SKILLS: usize = 50;
const MAX_ETIQUETAS: usize = 20;
const MAX_EJEMPLOS: usize = 10;
const MAX_MODOS: usize = 20;
const MAX_EXTENSIONES: usize = 20;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireInterface {
    pub url: String,
    pub protocol_binding: String,
    #[serde(default)]
    pub tenant: Option<String>,
    pub protocol_version: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireProvider {
    #[serde(default)]
    url: String,
    #[serde(default)]
    organization: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireExtension {
    #[serde(default)]
    uri: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    required: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireCapabilities {
    #[serde(default)]
    streaming: Option<bool>,
    #[serde(default)]
    push_notifications: Option<bool>,
    #[serde(default)]
    extensions: Vec<WireExtension>,
    #[serde(default)]
    extended_agent_card: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireSkill {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    examples: Vec<String>,
    #[serde(default)]
    input_modes: Vec<String>,
    #[serde(default)]
    output_modes: Vec<String>,
}

/// Tarjeta del cable. Solo se exigen `name`, `description`, `supportedInterfaces` y `version`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireAgentCard {
    pub name: String,
    pub description: String,
    pub supported_interfaces: Vec<WireInterface>,
    #[serde(default)]
    provider: Option<WireProvider>,
    pub version: String,
    #[serde(default)]
    documentation_url: Option<String>,
    #[serde(default)]
    capabilities: WireCapabilities,
    #[serde(default)]
    default_input_modes: Vec<String>,
    #[serde(default)]
    default_output_modes: Vec<String>,
    #[serde(default)]
    skills: Vec<WireSkill>,
    /// Solo interesa saber si hay requisitos; su contenido es de A8d.
    #[serde(default)]
    security_requirements: Vec<serde_json::Value>,
}

fn acotar(texto: &str, max: usize) -> String {
    if texto.len() <= max {
        return texto.to_string();
    }
    let mut fin = max;
    while !texto.is_char_boundary(fin) {
        fin -= 1;
    }
    format!("{}…", &texto[..fin])
}

fn acotar_lista(lista: &[String], max_elementos: usize, max_bytes: usize) -> Vec<String> {
    lista
        .iter()
        .take(max_elementos)
        .map(|elemento| acotar(elemento, max_bytes))
        .collect()
}

impl WireAgentCard {
    /// `true` si la tarjeta declara requisitos de seguridad: el remoto pide autenticación.
    pub fn requires_auth(&self) -> bool {
        !self.security_requirements.is_empty()
    }

    /// Primera interfaz JSON-RPC de la versión soportada, en el orden de preferencia de la
    /// tarjeta (§8.3.2). `None` si no hay ninguna; quien llama distingue por qué con
    /// [`WireAgentCard::ofrecidas`].
    pub fn interfaz_soportada(&self) -> Option<&WireInterface> {
        self.supported_interfaces.iter().find(|interfaz| {
            interfaz
                .protocol_binding
                .eq_ignore_ascii_case(BINDING_JSONRPC)
                && interfaz.protocol_version == VERSION_SOPORTADA
        })
    }

    /// Lo que la tarjeta ofrece, como «BINDING versión», para un mensaje de error legible.
    pub fn ofrecidas(&self) -> Vec<String> {
        self.supported_interfaces
            .iter()
            .take(MAX_INTERFACES)
            .map(|interfaz| {
                format!(
                    "{} {}",
                    acotar(&interfaz.protocol_binding, MAX_NOMBRE),
                    acotar(&interfaz.protocol_version, MAX_NOMBRE)
                )
            })
            .collect()
    }

    /// Convierte a la tarjeta del núcleo con cada campo acotado.
    pub fn into_core(&self) -> AgentCard {
        AgentCard {
            name: acotar(&self.name, MAX_NOMBRE),
            description: acotar(&self.description, MAX_TEXTO),
            supported_interfaces: self
                .supported_interfaces
                .iter()
                .take(MAX_INTERFACES)
                .map(|interfaz| AgentInterface {
                    url: acotar(&interfaz.url, MAX_TEXTO),
                    protocol_binding: acotar(&interfaz.protocol_binding, MAX_NOMBRE),
                    tenant: interfaz.tenant.as_deref().map(|t| acotar(t, MAX_NOMBRE)),
                    protocol_version: acotar(&interfaz.protocol_version, MAX_NOMBRE),
                })
                .collect(),
            provider: self.provider.as_ref().map(|proveedor| AgentProvider {
                url: acotar(&proveedor.url, MAX_TEXTO),
                organization: acotar(&proveedor.organization, MAX_NOMBRE),
            }),
            version: acotar(&self.version, MAX_NOMBRE),
            documentation_url: self
                .documentation_url
                .as_deref()
                .map(|url| acotar(url, MAX_TEXTO)),
            capabilities: AgentCapabilities {
                streaming: self.capabilities.streaming,
                push_notifications: self.capabilities.push_notifications,
                extensions: self
                    .capabilities
                    .extensions
                    .iter()
                    .take(MAX_EXTENSIONES)
                    .map(|extension| AgentExtension {
                        uri: acotar(&extension.uri, MAX_TEXTO),
                        description: acotar(&extension.description, MAX_TEXTO),
                        required: extension.required,
                        params: None,
                    })
                    .collect(),
                extended_agent_card: self.capabilities.extended_agent_card,
            },
            default_input_modes: acotar_lista(&self.default_input_modes, MAX_MODOS, MAX_NOMBRE),
            default_output_modes: acotar_lista(&self.default_output_modes, MAX_MODOS, MAX_NOMBRE),
            skills: self
                .skills
                .iter()
                .take(MAX_SKILLS)
                .map(|skill| AgentSkill {
                    id: acotar(&skill.id, MAX_NOMBRE),
                    name: acotar(&skill.name, MAX_NOMBRE),
                    description: acotar(&skill.description, MAX_TEXTO),
                    tags: acotar_lista(&skill.tags, MAX_ETIQUETAS, MAX_NOMBRE),
                    examples: acotar_lista(&skill.examples, MAX_EJEMPLOS, MAX_TEXTO),
                    input_modes: acotar_lista(&skill.input_modes, MAX_MODOS, MAX_NOMBRE),
                    output_modes: acotar_lista(&skill.output_modes, MAX_MODOS, MAX_NOMBRE),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TARJETA: &str = include_str!("../tests/fixtures/agent-card.sample.json");

    fn leida() -> WireAgentCard {
        serde_json::from_str(TARJETA).unwrap()
    }

    #[test]
    fn the_spec_sample_card_is_read() {
        let tarjeta = leida();

        assert_eq!(tarjeta.name, "GeoSpatial Route Planner Agent");
        assert_eq!(tarjeta.supported_interfaces.len(), 3);
        assert_eq!(tarjeta.skills.len(), 2);
        assert!(tarjeta.requires_auth());
    }

    #[test]
    fn the_first_supported_interface_is_the_jsonrpc_one_of_version_1_0() {
        let tarjeta = leida();

        let interfaz = tarjeta.interfaz_soportada().unwrap();

        assert_eq!(interfaz.url, "https://georoute-agent.example.com/a2a/v1");
    }

    #[test]
    fn a_card_offering_only_other_versions_or_bindings_has_no_supported_interface() {
        let mut tarjeta = leida();
        for interfaz in &mut tarjeta.supported_interfaces {
            interfaz.protocol_version = "0.3".into();
        }

        assert!(tarjeta.interfaz_soportada().is_none());
        assert_eq!(
            tarjeta.ofrecidas(),
            vec!["JSONRPC 0.3", "GRPC 0.3", "HTTP+JSON 0.3"]
        );
    }

    #[test]
    fn the_conversion_to_the_core_card_keeps_the_declared_content() {
        let core = leida().into_core();

        assert_eq!(core.name, "GeoSpatial Route Planner Agent");
        assert_eq!(core.capabilities.streaming, Some(true));
        assert_eq!(core.skills[0].id, "route-optimizer-traffic");
        assert_eq!(core.skills[0].tags.len(), 5);
        assert_eq!(
            core.default_input_modes,
            vec!["application/json", "text/plain"]
        );
        assert_eq!(
            core.provider.unwrap().organization,
            "Example Geo Services Inc."
        );
    }

    #[test]
    fn every_field_is_bounded_on_conversion() {
        let mut tarjeta = leida();
        tarjeta.name = "ñ".repeat(10_000);
        tarjeta.description = "x".repeat(50_000);
        tarjeta.skills = (0..500)
            .map(|n| WireSkill {
                id: format!("s{n}"),
                name: "n".into(),
                description: "d".into(),
                tags: vec!["t".into(); 500],
                examples: vec![],
                input_modes: vec![],
                output_modes: vec![],
            })
            .collect();

        let core = tarjeta.into_core();

        assert!(core.name.len() <= MAX_NOMBRE + '…'.len_utf8());
        assert!(core.name.is_char_boundary(core.name.len()));
        assert!(core.description.len() <= MAX_TEXTO + '…'.len_utf8());
        assert_eq!(core.skills.len(), MAX_SKILLS);
        assert_eq!(core.skills[0].tags.len(), MAX_ETIQUETAS);
    }
}
