//! Tarjeta del agente (`AgentCard`) y sus tipos auxiliares de A2A (`lf.a2a.v1`).
//!
//! Los campos y sus nombres son los del proto. Una tarjeta es un documento: tenerla no
//! obliga a servirla. Lo que el proto contempla y aquí se omite (seguridad, firmas, icono)
//! queda fuera hasta A8, y se señala en cada tipo afectado. La serialización de este
//! módulo es un formato interno de almacenamiento y de pruebas, no el del cable (A4).

use serde::{Deserialize, Serialize};

/// Organización que opera el agente. Mensaje `AgentProvider` del proto; `url` y
/// `organization` son obligatorios en él.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentProvider {
    pub url: String,
    pub organization: String,
}

/// Extensión de A2A que el agente declara. Mensaje `AgentExtension` del proto. `params`
/// es un valor libre y por eso se modela como `serde_json::Value`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentExtension {
    pub uri: String,
    pub description: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

/// Capacidades opcionales del agente. Mensaje `AgentCapabilities` del proto.
///
/// Los `Option<bool>` distinguen «no declarado» (`None`) de «declarado como falso»
/// (`Some(false)`), que es lo que el proto marca como `optional`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentCapabilities {
    #[serde(default)]
    pub streaming: Option<bool>,
    #[serde(default)]
    pub push_notifications: Option<bool>,
    #[serde(default)]
    pub extensions: Vec<AgentExtension>,
    #[serde(default)]
    pub extended_agent_card: Option<bool>,
}

/// Endpoint por el que se habla con el agente. Mensaje `AgentInterface` del proto.
///
/// Una tarjeta sin interfaces no es servible; ver `AgentCard::is_servable`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentInterface {
    pub url: String,
    pub protocol_binding: String,
    #[serde(default)]
    pub tenant: Option<String>,
    pub protocol_version: String,
}

/// Capacidad concreta que el agente anuncia. Mensaje `AgentSkill` del proto.
///
/// Los campos opcionales (`examples`, `input_modes`, `output_modes`) se deserializan vacíos
/// si faltan. Quedan fuera `security_requirements` hasta A8.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    #[serde(default)]
    pub examples: Vec<String>,
    #[serde(default)]
    pub input_modes: Vec<String>,
    #[serde(default)]
    pub output_modes: Vec<String>,
}

/// Documento con el que un agente se anuncia. Mensaje `AgentCard` del proto.
///
/// Quedan fuera `security_schemes`, `security_requirements`, `signatures` e `icon_url`:
/// sin servidor ni cliente (A8) no tienen consumidor, y modelarlos a medias sería inventar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentCard {
    pub name: String,
    pub description: String,
    pub supported_interfaces: Vec<AgentInterface>,
    #[serde(default)]
    pub provider: Option<AgentProvider>,
    pub version: String,
    #[serde(default)]
    pub documentation_url: Option<String>,
    pub capabilities: AgentCapabilities,
    pub default_input_modes: Vec<String>,
    pub default_output_modes: Vec<String>,
    pub skills: Vec<AgentSkill>,
}

impl AgentSkill {
    /// Skill con `id`, `name`, `description` y `tags`; el resto de campos queda vacío.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        tags: Vec<String>,
    ) -> Self {
        AgentSkill {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            tags,
            examples: vec![],
            input_modes: vec![],
            output_modes: vec![],
        }
    }
}

impl AgentCard {
    /// Tarjeta de un agente local: sin interfaces (no sirve nada todavía), capacidades sin
    /// declarar, modos de entrada y salida `["text/plain"]` porque hoy solo se intercambia
    /// texto, y sin skills. El proto no fija ese valor por defecto; es decisión nuestra.
    pub fn local(
        name: impl Into<String>,
        description: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        // Modos de texto plano: es lo único que el agente intercambia hoy.
        let modos_texto = vec!["text/plain".to_string()];
        AgentCard {
            name: name.into(),
            description: description.into(),
            supported_interfaces: vec![],
            provider: None,
            version: version.into(),
            documentation_url: None,
            capabilities: AgentCapabilities {
                streaming: None,
                push_notifications: None,
                extensions: vec![],
                extended_agent_card: None,
            },
            default_input_modes: modos_texto.clone(),
            default_output_modes: modos_texto,
            skills: vec![],
        }
    }

    /// Añade una skill al final de la lista y devuelve la tarjeta.
    pub fn with_skill(mut self, skill: AgentSkill) -> Self {
        self.skills.push(skill);
        self
    }

    /// `true` si la tarjeta tiene al menos una interfaz. Una tarjeta sin interfaces no es
    /// válida en el cable y no debe servirse hasta A8.
    pub fn is_servable(&self) -> bool {
        !self.supported_interfaces.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn interfaz() -> AgentInterface {
        AgentInterface {
            url: "https://ejemplo.invalid/a2a".into(),
            protocol_binding: "JSONRPC".into(),
            tenant: Some("equipo-demo".into()),
            protocol_version: "1.0".into(),
        }
    }

    #[test]
    fn card_roundtrips_through_serde() {
        let mut tarjeta =
            AgentCard::local("codeloom", "Agente de código", "0.1.0").with_skill(AgentSkill::new(
                "resumir",
                "Resumir",
                "Resume un texto",
                vec!["texto".to_string()],
            ));
        tarjeta.supported_interfaces.push(interfaz());
        tarjeta.provider = Some(AgentProvider {
            url: "https://ejemplo.invalid".into(),
            organization: "Equipo demo".into(),
        });
        tarjeta.documentation_url = Some("https://ejemplo.invalid/docs".into());
        tarjeta.capabilities = AgentCapabilities {
            streaming: Some(true),
            push_notifications: Some(false),
            extensions: vec![AgentExtension {
                uri: "urn:ejemplo:extension".into(),
                description: "Extensión de prueba".into(),
                required: false,
                params: Some(json!({"nivel": 2})),
            }],
            extended_agent_card: None,
        };

        let json = serde_json::to_string(&tarjeta).unwrap();
        let vuelta: AgentCard = serde_json::from_str(&json).unwrap();
        assert_eq!(vuelta, tarjeta);
    }

    #[test]
    fn local_card_has_declared_defaults() {
        let tarjeta = AgentCard::local("codeloom", "Agente de código", "0.1.0");
        assert_eq!(tarjeta.name, "codeloom");
        assert_eq!(tarjeta.description, "Agente de código");
        assert_eq!(tarjeta.version, "0.1.0");
        assert!(tarjeta.supported_interfaces.is_empty());
        assert_eq!(tarjeta.provider, None);
        assert_eq!(tarjeta.documentation_url, None);
        assert_eq!(
            tarjeta.capabilities,
            AgentCapabilities {
                streaming: None,
                push_notifications: None,
                extensions: vec![],
                extended_agent_card: None,
            }
        );
        assert_eq!(tarjeta.default_input_modes, vec!["text/plain".to_string()]);
        assert_eq!(tarjeta.default_output_modes, vec!["text/plain".to_string()]);
        assert!(tarjeta.skills.is_empty());
    }

    #[test]
    fn local_card_is_not_servable() {
        let tarjeta = AgentCard::local("codeloom", "Agente de código", "0.1.0");
        assert!(!tarjeta.is_servable());
    }

    #[test]
    fn card_with_an_interface_is_servable() {
        let mut tarjeta = AgentCard::local("codeloom", "Agente de código", "0.1.0");
        tarjeta.supported_interfaces.push(interfaz());
        assert!(tarjeta.is_servable());
    }

    #[test]
    fn with_skill_appends_a_skill_with_its_tags() {
        let tarjeta = AgentCard::local("codeloom", "Agente de código", "0.1.0")
            .with_skill(AgentSkill::new(
                "uno",
                "Uno",
                "Primera skill",
                vec!["a".to_string(), "b".to_string()],
            ))
            .with_skill(AgentSkill::new(
                "dos",
                "Dos",
                "Segunda skill",
                vec!["c".to_string()],
            ));
        assert_eq!(tarjeta.skills.len(), 2);
        assert_eq!(tarjeta.skills[0].id, "uno");
        assert_eq!(
            tarjeta.skills[0].tags,
            vec!["a".to_string(), "b".to_string()]
        );
        assert_eq!(tarjeta.skills[1].id, "dos");
        assert_eq!(tarjeta.skills[1].tags, vec!["c".to_string()]);
    }

    #[test]
    fn capabilities_distinguish_undeclared_from_false() {
        let sin_declarar = AgentCapabilities {
            streaming: None,
            push_notifications: None,
            extensions: vec![],
            extended_agent_card: None,
        };
        let falso = AgentCapabilities {
            streaming: Some(false),
            ..sin_declarar.clone()
        };
        assert_ne!(sin_declarar, falso);

        let vuelta_sin: AgentCapabilities =
            serde_json::from_str(&serde_json::to_string(&sin_declarar).unwrap()).unwrap();
        let vuelta_falso: AgentCapabilities =
            serde_json::from_str(&serde_json::to_string(&falso).unwrap()).unwrap();
        assert_eq!(vuelta_sin.streaming, None);
        assert_eq!(vuelta_falso.streaming, Some(false));
        assert_ne!(vuelta_sin, vuelta_falso);

        // Un JSON sin el campo cuenta como no declarado, no como falso.
        let desde_json: AgentCapabilities = serde_json::from_str("{}").unwrap();
        assert_eq!(desde_json.streaming, None);
    }

    #[test]
    fn skill_without_optional_fields_roundtrips() {
        let skill = AgentSkill::new(
            "resumir",
            "Resumir",
            "Resume un texto",
            vec!["texto".to_string()],
        );
        assert!(skill.examples.is_empty());
        assert!(skill.input_modes.is_empty());
        assert!(skill.output_modes.is_empty());

        let json = serde_json::to_string(&skill).unwrap();
        let vuelta: AgentSkill = serde_json::from_str(&json).unwrap();
        assert_eq!(vuelta, skill);

        // JSON mínimo: sin `examples` ni modos, debe deserializar sin error.
        let minimo: AgentSkill = serde_json::from_str(
            r#"{"id":"resumir","name":"Resumir","description":"Resume un texto","tags":["texto"]}"#,
        )
        .unwrap();
        assert_eq!(minimo, skill);
    }
}
