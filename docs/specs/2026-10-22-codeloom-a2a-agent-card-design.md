# La tarjeta del agente (`AgentCard`) en el núcleo

Estado: propuesta del 2026-10-22 (tarea A6 de `docs/plans/2026-10-22-codeloom-a2a-roadmap.md`).
Plan: `docs/plans/2026-10-22-codeloom-a2a-agent-card.md`. Continúa
`2026-10-22-codeloom-a2a-protocol-types-design.md` (A5). Fuente normativa: `a2a.proto`
(`lf.a2a.v1`), mensajes `AgentCard`, `AgentProvider`, `AgentCapabilities`,
`AgentExtension`, `AgentSkill` y `AgentInterface`. El mapeo perfil→tarjeta ya está
razonado en `2026-10-15-loom-agent-profile-a2a.md` §4.

## Intención

La tarjeta es el documento con el que un agente se anuncia: quién es, qué sabe hacer
(`skills[]` con `tags`) y cómo se le habla. Es la base de la delegación por perfil
(A12). Se adoptan sus tipos tal cual en `codeloom-core`, sin red ni servidor: una
tarjeta es un documento, y tenerla no obliga a servirla.

## Qué dice el proto (literal)

- `AgentCard { name*, description*, supported_interfaces*, provider, version*, documentation_url, capabilities*, security_schemes, security_requirements, default_input_modes*, default_output_modes*, skills*, signatures, icon_url }` (`*` = REQUIRED).
- `AgentSkill { id*, name*, description*, tags*, examples, input_modes, output_modes, security_requirements }`.
- `AgentInterface { url*, protocol_binding*, tenant, protocol_version* }`.
- `AgentCapabilities { streaming, push_notifications, extensions, extended_agent_card }`, todos opcionales.
- `AgentExtension { uri, description, required, params }`; `AgentProvider { url*, organization* }`.

## Decisiones

### Respaldadas por el protocolo

- Los campos y sus nombres en snake_case son los del proto.
- `AgentCapabilities` conserva los `Option<bool>` (el proto los marca `optional`: «no
  declarado» y «falso» son cosas distintas).

### Lecturas o simplificaciones nuestras (rotuladas)

- **Quedan fuera, y se dice:** `security_schemes`, `security_requirements` (en la tarjeta
  y en la skill), `signatures` e `icon_url`. Sin servidor ni cliente (A8) no tienen
  consumidor, y modelarlos a medias sería inventar. Entran con A8.
- **Una tarjeta local no tiene interfaz.** El proto marca `supported_interfaces` como
  obligatorio; un agente local no sirve nada, así que su lista va vacía. **Una tarjeta
  con la lista vacía no es válida en el cable y no debe servirse** hasta A8.
- **Modos por defecto de un agente local:** `default_input_modes` y
  `default_output_modes` = `["text/plain"]`, porque hoy el agente solo intercambia texto.
  El proto no fija un valor por defecto.
- **Serde interno:** el formato es de almacenamiento y de pruebas, no el del cable (A4).

## No objetivos

- Servir o descargar tarjetas (`/.well-known/agent-card.json`), tarjeta extendida: A8.
- Cargar `AgentProfile` desde fichero: no existe aún código de perfiles; es otra pieza.
- Elegir agente por etiquetas: A12.

## Anti-objetivos

- Campos propios en la tarjeta. Lo local (scope, delegación) **no viaja**, como ya fija
  la spec del perfil; si hiciera falta anunciar algo propio, iría como `AgentExtension`.

## Qué se deja al implementador

Nombre del módulo (`card.rs` sugerido) y cómo se componen los ayudantes de construcción.

## Verificación

Pruebas puras: ida y vuelta serde de una tarjeta completa; constructor local con los
valores por defecto declarados; `with_skill` añade una skill con sus `tags`; capacidades
sin declarar frente a `Some(false)`; una tarjeta local tiene `supported_interfaces`
vacío y lo reporta un método (`is_servable()` = false).

## Estado (2026-10-22)

**Implementado.** Commit `77b8de1` (`feat(core): los tipos de la tarjeta de agente de A2A`):
módulo `crates/codeloom-core/src/card.rs`, declarado en `lib.rs`. Contiene los seis tipos
de «Qué dice el proto», `AgentSkill::new`, `AgentCard::local`, `AgentCard::with_skill` y
`AgentCard::is_servable`, con siete tests: `card_roundtrips_through_serde`,
`local_card_has_declared_defaults`, `local_card_is_not_servable`,
`card_with_an_interface_is_servable`, `with_skill_appends_a_skill_with_its_tags`,
`capabilities_distinguish_undeclared_from_false` y `skill_without_optional_fields_roundtrips`.
El plan es `docs/plans/2026-10-22-codeloom-a2a-agent-card.md`; esta spec y el plan se
commitearon en `cf82838`.

**Decisiones tomadas al implementar** (además de las de arriba):

- Los campos `Option` llevan `#[serde(default)]`: un JSON sin ellos se deserializa como `None`.
- Los `Vec` obligatorios del proto (`supported_interfaces`, `tags`, los modos por defecto y
  `skills`) no tienen valor por defecto de serde. Un JSON que no los trae falla al
  deserializar, como marca el proto. Los opcionales de la skill (`examples`, `input_modes`,
  `output_modes`) sí lo tienen.
- `AgentCapabilities` no deriva `Default`; `AgentCard::local` construye sus campos a mano.
- `is_servable()` solo comprueba que haya al menos una interfaz. No valida `url`,
  `protocol_binding` ni `protocol_version`.

**Limitaciones reales:**

- Un `Option<serde_json::Value>` con `Some(null)` no sobrevive la ida y vuelta: serde lo lee
  como `None`. Pasa en `AgentExtension::params` igual que en los `metadata` de `a2a.rs`.
  No consta ningún caso que dependa de esa distinción; si apareciera, habría que cambiar el tipo.
- El test `capabilities_distinguish_undeclared_from_false` **pasó ya en la fase roja**,
  porque se apoya solo en serde derivado y no toca ningún cuerpo dejado en `todo!()`. Por
  tanto no demuestra por sí mismo que la prueba falle sin el código; cabe señalar que
  comprueba una propiedad que garantiza el derivado.
- No existe código de `AgentProfile`. El formato solo está en la spec del perfil del 15 de
  octubre, así que la tarjeta no se puede sintetizar aún desde un fichero.
- No se sirve ni se descarga ninguna tarjeta (A7 y A8), y no hay validación de tarjetas recibidas.
- Quedan fuera `security_schemes`, `security_requirements` (en la tarjeta y en la skill),
  `signatures` e `icon_url`. Entran con A8.

**Pendiente:**

- A8: seguridad, firmas e icono de la tarjeta. Si la firma se implementa con JCS, habrá que
  distinguir «no puesto» de «puesto con valor por defecto» (lectura nuestra de la spec del
  protocolo, sección 8.4).
- Cargador de `AgentProfile` desde fichero, prerrequisito de A12. Queda como entrada A13 de
  la hoja de ruta (`docs/plans/2026-10-22-codeloom-a2a-roadmap.md`).
