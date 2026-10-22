# Registro de agentes con fuentes intercambiables

Estado: propuesta del 2026-10-22 (tarea A13 de `docs/plans/2026-10-22-codeloom-a2a-roadmap.md`).
Prerrequisito de A12 (delegar por perfil). Se apoya en la tarjeta de A6
(`2026-10-22-codeloom-a2a-agent-card-design.md`) y en el formato de perfil de
`2026-10-15-loom-agent-profile-a2a.md` §2. Decisiones del usuario del 2026-10-22: perfiles
globales y por proyecto, recarga en caliente, y tres niveles de agente.

## Intención

Quien delega debe poder consultar «qué agentes hay» sin saber de dónde vienen. Hoy
`SubagentLauncher::start` recibe solo `parent`, `label`, `prompt` y `cancel`: no hay forma
de elegir agente, modelo ni esfuerzo. Se introduce un **registro** que reúne agentes de
varias fuentes y los expone como **tarjetas** (`AgentCard`) más lo necesario para
ejecutarlos. Esta spec cubre el registro, la fuente de ficheros, el agente por defecto, la
recarga en caliente y el hueco por el que `start` recibe un agente. **No** cubre cómo se
elige (A12) ni los agentes remotos (A8), aunque el diseño los admite.

## Qué dice A2A (y qué no)

- §8.2: tres vías de descubrimiento de tarjetas: «Well-Known URI», «Registries/Catalogs» y
  «Direct Configuration: Pre-configured Agent Card URLs or content». Nuestros ficheros son
  la tercera; los agentes remotos serán la primera (A8).
- **No consta** ninguna ubicación en disco para guardar agentes, ni formato de perfil, ni
  campo de modelo o esfuerzo en la tarjeta (ver A12). Todo ello es decisión nuestra y se
  rotula como propia.

## Los tres niveles

| Nivel | Qué es | Modelo y esfuerzo |
|---|---|---|
| 1. Por defecto | Agente genérico local, siempre presente | Heredados del padre |
| 2. Propios | Ficheros `.md`, globales o de proyecto | Los del perfil; si faltan, heredados |
| 3. Externos | Agentes remotos por HTTP (A8) | No se pueden enviar; los decide el agente remoto |

Modelo y esfuerzo son configuración de **ejecución local**: no viajan en la tarjeta.

## Modelo de datos

```rust
pub enum AgentOrigin {
    BuiltIn,
    File { path: PathBuf, level: ProfileLevel },   // ProfileLevel::{Global, Project}
    Remote { card_url: String },                   // A8; el tipo existe, nada lo produce aún
}

pub struct AgentEntry {
    pub name: String,            // identificador único dentro del registro
    pub origin: AgentOrigin,
    pub card: AgentCard,         // A6: lo que se anuncia
    pub execution: Execution,    // lo local, que no viaja en la tarjeta
}

pub enum Execution {
    Local(LocalSpec),            // persona, model, effort, max_turns, max_depth
    Remote,                      // se habla por transporte (A8)
}

pub trait AgentSource {
    fn id(&self) -> &str;
    fn load(&self) -> SourceLoad;     // SourceLoad { entries: Vec<AgentEntry>, problems: Vec<LoadProblem> }
}
```

`AgentRegistry` reúne fuentes, aplica la precedencia y entrega **instantáneas inmutables**
(`Arc<RegistrySnapshot>`). Una tarea en curso conserva la instantánea con la que arrancó:
editar un fichero **no** altera a un subagente que ya está trabajando.

## Decisiones

### Lecturas o simplificaciones nuestras (rotuladas)

- **Ubicación.** Global: `~/.agents/agents/` (donde la spec de skills ya sitúa las
  skills); por proyecto: `.agents/agents/` en la raíz del repo. Ambas rutas configurables
  en `codeloom.toml`. A2A no recomienda ninguna.
- **Un fichero, un agente:** `<nombre>.md`, con frontmatter YAML y cuerpo markdown. El
  cuerpo es el prompt de sistema (persona). El `name` del frontmatter debe coincidir con el
  nombre del fichero sin extensión; el nombre admite solo `a-z`, `0-9` y `-`, porque se usa
  como identificador y como parte de una ruta.
- **Precedencia:** proyecto > global > integrado. Si dos agentes comparten nombre, gana el
  de mayor precedencia y el otro se anota como `LoadProblem::Shadowed`, visible, no
  silenciado.
- **El agente por defecto se llama `default`** y es integrado. Un `default.md` de proyecto
  o global lo redefine. Es el único nivel que **nunca falla**: la caída al agente por
  defecto debe ser visible y razonada (la elección es A12).
- **Campos que se interpretan ahora:** `name`, `description`, `version`, `persona`,
  `model`, `effort`, `max_turns`, `max_depth`. **Campos que se leen y se conservan pero
  no se aplican todavía:** `scope`, `delegation`, `transport`, `a2a`. Un perfil que declare
  `scope` recibe un aviso «declarado y no aplicado»: aparentar una allowlist que no se
  cumple sería peor que no tenerla.
- **Tolerancia:** campos desconocidos se ignoran con aviso; un perfil incompleto usa los
  valores por defecto de la spec del perfil. Nada falla por un campo nuevo.
- **Errores no destructivos:** si un fichero se rompe al editarlo, el registro **conserva
  la última versión válida** y publica el problema. Un fichero en mitad de un guardado no
  puede hacer desaparecer al agente.
- **`model` desconocido:** el perfil se carga con aviso; falla al **usarse** (`start`),
  con error claro y sin crear tarea, como cualquier validación de entrada.
- **La tarjeta se sintetiza** desde el perfil según la tabla de mapeo de la spec del perfil
  §4: `name`, `description`, `version`; `skills[]` y `tags` a partir de lo declarado.
  Persona, `scope`, `model`, `effort` y delegación **no viajan**. Una tarjeta local sigue
  sin interfaces y no es servible (A6).
- **Recarga en caliente** con vigilancia de ficheros y *debounce*, o sondeo de fecha de
  modificación si la vigilancia no estuviera disponible; lo decide el implementador. Nunca
  puede bloquear ni hacer parpadear la interfaz (prioridad de fluidez del compositor).
- **Confianza.** Un perfil de proyecto llega con el repositorio y es contenido de un
  tercero: puede traer un prompt de sistema o elegir un modelo caro. **No puede ampliar
  permisos**: toda herramienta pasa igualmente por `PermissionService`. Se muestra siempre
  su origen.
- **Aprobación de los agentes de proyecto (decisión del usuario, 2026-10-22: sí).** Un
  agente de nivel `Project` **no se puede usar** hasta que el usuario lo aprueba. Mientras
  tanto figura en el registro como `pendiente` y `start` lo rechaza con un error que lo
  dice. Los de nivel `Global` y el integrado no la requieren: son del propio usuario.
  Lectura nuestra de los detalles: la aprobación se identifica por (raíz canónica del
  proyecto, nombre, huella del contenido), de modo que **cualquier cambio del fichero la
  invalida**; así un `git pull` que modifique un agente no se cuela. Se guarda en el
  directorio de datos del usuario (`~/.codeloom`), nunca en el repositorio. Una opción de
  `codeloom.toml` permite confiar en todo un proyecto (`trust_project_agents`); por
  defecto, desactivada. Esta spec solo exige la API de aprobar y revocar; el diálogo de la
  interfaz es posterior.

## Cambio en `SubagentLauncher::start`

Recibe un parámetro opcional de agente (`agent: Option<String>`; `None` = `default`). El
lanzador resuelve el nombre en la instantánea vigente y aplica a la sesión hija: prompt de
sistema (persona), modelo y esfuerzo (con herencia del padre si faltan), `max_turns` y
`max_depth`. Nombre inexistente: error de validación, sin tarea. Qué agente se elige lo
decide quien llama (A12 lo automatiza).

## Eventos y observabilidad

Evento nuevo `AgentRegistryChanged` en el bus existente (altas, bajas, cambios, problemas),
sin canal paralelo. La interfaz podrá mostrarlo; mostrarlo **no** es objetivo de esta spec.
Cada tarea registra con qué agente y qué origen arrancó (campo del descriptor).

## No objetivos

- Elegir agente por tarea (A12) y el modelo padre que decide.
- Agentes remotos, descarga de tarjetas, `A2aRemote` (A8). El tipo `Remote` existe para no
  rehacer el registro, pero nada lo produce.
- Aplicar `scope`/`delegation` (queda declarado y no aplicado).
- Editar agentes desde la interfaz. Los ficheros se editan a mano o con cualquier editor;
  la edición dentro de la TUI es posterior y reutilizará la misma recarga.

## Anti-objetivos

- Que el registro conozca detalles de una fuente concreta: añadir la remota no debe tocarlo.
- Que una edición a medias, un fichero ilegible o un duplicado dejen la interfaz sin
  agentes o rompan una tarea en curso.
- Que un perfil de proyecto eleve permisos.

## Qué se deja al implementador

Parser de YAML (el que se elija debe estar mantenido; informar de la elección y de la
versión verificada), vigilancia de ficheros frente a sondeo, forma exacta de `LoadProblem`
y del evento, y nombres de módulos (`registry.rs`, `profile.rs` sugeridos).

## Verificación

Pruebas puras, sin red: lectura de un perfil completo y de uno mínimo; tolerancia a campos
desconocidos; rechazo de nombres inválidos (mayúsculas, `..`, separadores) y de `name`
distinto del nombre de fichero; precedencia proyecto > global > integrado y
`Shadowed`; `default` redefinido por fichero; síntesis de tarjeta sin persona ni modelo;
recarga: fichero nuevo, editado, borrado y roto (conserva la última versión válida);
instantánea inmutable frente a una recarga con una tarea en curso; `start` con agente
inexistente sin crear tarea; `start` con perfil que aplica persona, modelo y esfuerzo, y con
herencia cuando faltan. Validación visual en terminal real: pendiente.

## Preguntas abiertas

- (Resuelta) Aprobación de los agentes de proyecto: sí, ver «Confianza».
- Nombre y ubicación exactos de la clave de configuración de las rutas en `codeloom.toml`.
- Qué formato de `skills[]` de la tarjeta se deriva cuando el perfil solo trae `description`
  y `persona` (¿una skill sintética con la descripción?). Hasta decidirlo, la lista va vacía.

## Estado (2026-10-22)

Implementado en los ocho commits de A13, en orden cronológico:

- `1f4787a`: lectura y validación de perfiles de agente (`registry/profile.rs`).
- `73a4e41`: registro con fuente de ficheros y agente `default` (`registry/mod.rs`, `registry/source.rs`).
- `a492f73`: la persona del perfil viaja con la entrada y compone el prompt.
- `128cfcd`: recarga en caliente del registro (`registry/watch.rs`).
- `89329b7`: aprobación de los agentes de proyecto (`registry/trust.rs`).
- `f3f0240`: el descriptor del subagente recuerda con qué agente arrancó.
- `27dd8e4`: el lanzador de subagentes admite elegir agente del registro.
- `05dc0ac`: el registro de agentes se construye al arrancar.

### Decisiones que esta spec no fijaba

- **Parser YAML:** `serde_yaml_bw` 2.5.8. Se descartaron `serde_yaml` (archivado) y `serde_yml` (obsoleto).
- **Recarga por sondeo, no por `notify`.** Cada 250 ms se comparan fecha de modificación y tamaño de los `*.md` de los directorios vigilados. El mismo intervalo hace de *debounce*. Corre en un hilo propio, nunca en el de la interfaz, y no añade ninguna dependencia.
- **Aprobación de agentes de proyecto.** La huella es SHA-256 (`sha2` 0.10) del contenido. La clave de aprobación es la terna (raíz canónica del proyecto, nombre, huella). El almacén vive en `<home>/.codeloom/agent-trust.json` y se escribe de forma atómica; un JSON corrupto cuenta como «nada aprobado». Los remotos se deniegan por defecto. No hay bloqueo entre procesos.
- **Orden de validación de `start`**, siempre sin crear tarea: agente inexistente, agente remoto, agente pendiente de aprobación y modelo fuera del catálogo.
- **Persona.** Se añade tras el contexto del proyecto (`AGENTS.md` y similares) y no lo sustituye. Si hay persona, antepone las líneas `Rol:`, `Objetivo:` y `Contexto:` (en español) al cuerpo.
- **`max_turns`** se aplica como `max_steps` de la sesión hija y **sustituye** al del padre, aunque sea mayor.
- **`max_depth`** se lee y no se aplica. La sesión hija no recibe la herramienta `subagent`, así que no puede anidar subagentes.
- **Descriptor.** Guarda `agent` y `agent_origin` (`built-in`, `global:<ruta>`, `project:<ruta>` o `remote:<url>`). Las columnas se añaden de forma idempotente; las filas antiguas leen `None`.
- **Configuración.** Sección opcional `[agents]` con `global_dir`, `project_dir` y `trust_project_agents`. Es un tipo propio (`AgentsConfig`), aparte de `CodeLoomConfig`.
- **Sin directorio home** solo se usan la fuente de proyecto y la integrada.
- **Evento** `LiveEvent::AgentRegistryChanged`, con altas, bajas, cambios y número de problemas. Solo se emite si hubo diferencias.
- **Última versión válida.** Si un fichero se rompe, se conserva la entrada anterior y se anota el problema. Si se borra, la entrada se retira.

### Limitaciones reales

- `scope`, `delegation`, `transport` y `a2a` se leen y se declaran, pero no se aplican. Un perfil que los declara recibe el aviso «declarado y no aplicado». `max_depth` tampoco se aplica, y se lee sin aviso.
- **Sin diálogo de aprobación en la interfaz.** Hoy un agente de proyecto solo se usa con `trust_project_agents = true` o aprobándolo por API.
- `AgentRegistryChanged` se emite, pero la TUI lo recibe y no lo muestra.
- **Sin fuente remota** (A8). `Execution::Remote` existe y `start` lo rechaza con «agente remoto no disponible».
- **Sin edición desde la TUI.** Los ficheros se editan a mano o con cualquier editor.
- `skills[]` de la tarjeta sintetizada va vacío, como dice la pregunta abierta de esta spec.
- **Recarga por sondeo.** Un cambio que deja la misma fecha de modificación y el mismo tamaño no se detecta.
- Un `default.md` de proyecto o global se anota como `Shadowed` del integrado, aunque sea una redefinición legítima.
- Un enlace simbólico roto `*.md` se ignora en silencio.
- La regla del directorio ilegible no tiene test.
- No hay test de extremo a extremo de `AgentLoop` con un agente del registro.
- Preexistente: si no hay home, `main.rs` usa el cwd como directorio de datos, así que `sessions.db` queda dentro del repositorio.
- La vigilancia también arranca en modo `--print`.
- Validación visual en terminal real: pendiente.

### Cómo usarlo

Perfil global mínimo, en `~/.agents/agents/rust.md` (el nombre del fichero debe coincidir con `name`):

```markdown
---
name: rust
description: Especialista en Rust para cambios en el núcleo
model: <id de un modelo del catálogo>
effort: low
---

Eres un especialista en Rust. Cambia solo lo que la tarea pide y ejecuta
las pruebas del crate afectado antes de dar el trabajo por terminado.
```

La herramienta `subagent` acepta `prompt` (obligatorio) y `agent` (opcional: nombre de un agente del registro; omitido equivale a `default`):

```json
{"prompt": "Revisa el manejo de errores de crates/codeloom-core/src/registry/trust.rs", "agent": "rust"}
```

Los perfiles de proyecto viven en `.agents/agents/` de la raíz del repositorio. Un perfil de proyecto solo se usa si `trust_project_agents = true` o si se ha aprobado por API; en otro caso, `start` lo rechaza con «pendiente de aprobación».
