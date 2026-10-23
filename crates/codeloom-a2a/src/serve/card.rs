//! La tarjeta que el servidor anuncia en `/.well-known/agent-card.json` (A8b).
//!
//! Cabe señalar que se construye con los nombres del cable v1.0 (camelCase) y que declara
//! `pushNotifications` y `extendedAgentCard` a `false` y `streaming` a `true` (A8c): lo que no se soporta no se
//! anuncia. La seguridad que declara es la que el servidor exige de verdad.

use codeloom_core::card::AgentCard;
use codeloom_core::registry::content_hash;
use serde_json::{json, Value};

use super::config::ServeAuthScheme;
use crate::card::{BINDING_JSONRPC, VERSION_SOPORTADA};

const MAX_NOMBRE: usize = 200;
const MAX_TEXTO: usize = 2_000;
/// Nombre del esquema de seguridad que se declara en la tarjeta.
pub const NOMBRE_ESQUEMA: &str = "codeloom";

/// Tarjeta servida y su `ETag`.
#[derive(Debug, Clone, PartialEq)]
pub struct ServedCard {
    pub json: Value,
    pub etag: String,
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

/// Tarjeta del agente servido. `interface_url` es la URL JSON-RPC pública; `scheme`, el esquema
/// de autenticación que el servidor exige.
pub fn served_card(card: &AgentCard, interface_url: &str, scheme: &ServeAuthScheme) -> ServedCard {
    let esquema = match scheme {
        ServeAuthScheme::Bearer => {
            json!({ "httpAuthSecurityScheme": { "scheme": "Bearer" } })
        }
        ServeAuthScheme::ApiKey { name } => {
            json!({ "apiKeySecurityScheme": { "location": "header", "name": name } })
        }
    };
    let skills: Vec<Value> = card
        .skills
        .iter()
        .take(50)
        .map(|skill| {
            json!({
                "id": acotar(&skill.id, MAX_NOMBRE),
                "name": acotar(&skill.name, MAX_NOMBRE),
                "description": acotar(&skill.description, MAX_TEXTO),
                "tags": skill.tags.iter().take(20).map(|t| acotar(t, MAX_NOMBRE)).collect::<Vec<_>>(),
            })
        })
        .collect();
    let json = json!({
        "name": acotar(&card.name, MAX_NOMBRE),
        "description": acotar(&card.description, MAX_TEXTO),
        "supportedInterfaces": [{
            "url": interface_url,
            "protocolBinding": BINDING_JSONRPC,
            "protocolVersion": VERSION_SOPORTADA,
        }],
        "version": acotar(&card.version, MAX_NOMBRE),
        "capabilities": {
            "streaming": true,
            "pushNotifications": false,
            "extendedAgentCard": false,
        },
        "securitySchemes": { NOMBRE_ESQUEMA: esquema },
        // Como el SDK oficial: una lista de ámbitos vacía se omite.
        "securityRequirements": [{ "schemes": { NOMBRE_ESQUEMA: {} } }],
        "defaultInputModes": ["text/plain"],
        "defaultOutputModes": ["text/plain"],
        "skills": skills,
    });
    let etag = format!("\"{}\"", &content_hash(&json.to_string())[..32]);
    ServedCard { json, etag }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::WireAgentCard;
    use crate::security::{AuthPlan, CardSecurity};
    use std::collections::BTreeMap;

    fn tarjeta() -> AgentCard {
        AgentCard::local("default", "Agente de CodeLoom", "1.0.0")
    }

    #[test]
    fn the_served_card_declares_no_stream_push_or_extended_card() {
        let servida = served_card(
            &tarjeta(),
            "http://127.0.0.1:7077/a2a",
            &ServeAuthScheme::Bearer,
        );

        assert_eq!(servida.json["capabilities"]["streaming"], true);
        assert_eq!(servida.json["capabilities"]["pushNotifications"], false);
        assert_eq!(servida.json["capabilities"]["extendedAgentCard"], false);
        assert_eq!(
            servida.json["supportedInterfaces"][0]["url"],
            "http://127.0.0.1:7077/a2a"
        );
        assert_eq!(
            servida.json["supportedInterfaces"][0]["protocolVersion"],
            "1.0"
        );
    }

    #[test]
    fn our_own_client_reads_the_served_card_and_knows_how_to_authenticate() {
        for scheme in [
            ServeAuthScheme::Bearer,
            ServeAuthScheme::ApiKey {
                name: "X-Api-Key".into(),
            },
        ] {
            let servida = served_card(&tarjeta(), "http://127.0.0.1:7077/a2a", &scheme);

            let leida: WireAgentCard = serde_json::from_value(servida.json.clone()).unwrap();
            assert!(leida.interfaz_soportada().is_some());
            assert!(leida.requires_auth());

            let seguridad = CardSecurity::from_card_json(&servida.json);
            let credenciales = BTreeMap::from([(NOMBRE_ESQUEMA.to_string(), "V".to_string())]);
            let plan = AuthPlan::choose(&seguridad, &credenciales, &|_| Some("tok".into()));
            assert!(plan.is_ok(), "{scheme:?}: {plan:?}");
            assert!(!plan.unwrap().is_open());
        }
    }

    #[test]
    fn the_security_scheme_names_are_the_wire_ones() {
        let bearer = served_card(&tarjeta(), "http://x/a2a", &ServeAuthScheme::Bearer);
        assert_eq!(
            bearer.json["securitySchemes"][NOMBRE_ESQUEMA]["httpAuthSecurityScheme"]["scheme"],
            "Bearer"
        );
        let clave = served_card(
            &tarjeta(),
            "http://x/a2a",
            &ServeAuthScheme::ApiKey { name: "K".into() },
        );
        let esquema = &clave.json["securitySchemes"][NOMBRE_ESQUEMA]["apiKeySecurityScheme"];
        assert_eq!(esquema["location"], "header");
        assert_eq!(esquema["name"], "K");
    }

    #[test]
    fn the_etag_changes_with_the_content_and_is_stable_otherwise() {
        let a = served_card(&tarjeta(), "http://x/a2a", &ServeAuthScheme::Bearer);
        let igual = served_card(&tarjeta(), "http://x/a2a", &ServeAuthScheme::Bearer);
        let otra_url = served_card(&tarjeta(), "http://y/a2a", &ServeAuthScheme::Bearer);

        assert_eq!(a.etag, igual.etag);
        assert_ne!(a.etag, otra_url.etag);
        assert!(a.etag.starts_with('"') && a.etag.ends_with('"'));
    }

    #[test]
    fn long_texts_are_bounded() {
        let mut tarjeta = tarjeta();
        tarjeta.description = "d".repeat(10_000);
        tarjeta.name = "n".repeat(10_000);

        let servida = served_card(&tarjeta, "http://x/a2a", &ServeAuthScheme::Bearer);

        assert!(servida.json["description"].as_str().unwrap().len() <= MAX_TEXTO + 4);
        assert!(servida.json["name"].as_str().unwrap().len() <= MAX_NOMBRE + 4);
    }
}
