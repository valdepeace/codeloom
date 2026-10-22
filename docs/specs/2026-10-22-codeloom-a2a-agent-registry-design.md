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
  su origen. Si se pide aprobación al cargar por primera vez un agente de proyecto queda
  como pregunta abierta.

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

- ¿Aprobación la primera vez que se carga un agente de proyecto?
- Nombre y ubicación exactos de la clave de configuración de las rutas en `codeloom.toml`.
- Qué formato de `skills[]` de la tarjeta se deriva cuando el perfil solo trae `description`
  y `persona` (¿una skill sintética con la descripción?). Hasta decidirlo, la lista va vacía.
