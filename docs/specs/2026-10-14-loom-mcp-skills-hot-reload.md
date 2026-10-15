# CodeLoom — Spec: carga en caliente de MCP y skills (hot-reload)

> Estado: **borrador para revisión**.
>
> Fecha: 2026-10-14
>
> Depende de: `docs/specs/2026-10-13-loom-design.md` (§3 concurrencia, §4.4 bus,
> §5 tools). Contexto: `docs/reference/analisis-herdr-crush-dsh.md`.

## 0. Por qué hoy hay que salir y volver a arrancar

Todos los agentes de código (crush, Claude Code, Cursor, etc.) comparten las
mismas causas. No es una limitación del protocolo MCP: **la spec de MCP ya
define notificaciones de cambio de lista** (`notifications/tools/list_changed`,
`notifications/resources/list_changed`, `notifications/prompts/list_changed`).
El protocolo anticipó el hot-reload; lo que falta es el **consumidor**.

Razones observadas:

1. **Ciclo de vida del servidor**. Sobre `stdio`, cada servidor MCP es un
   proceso hijo que la app arranca al boot y cuyo ciclo de vida queda atado al
   de la app. La mayoría no tiene cableado para matar/relanzar/reconectar un
   servidor a mitad de sesión.
2. **El registro de tools se construye una vez**. En crush `buildTools`
   (`coordinator.go:1411-1535`) arma la lista al arrancar y el loop la recibe
   como un `slice` estático. No hay seam de mutación ni watcher de ficheros.
3. **Los esquemas de tools van dentro de la petición**. El conjunto de tools se
   serializa al frente de cada request (o del system prompt). Cambiarlo a mitad
   de sesión cambia el *prefijo* de la petición, lo que **invalida el prompt
   cache** del provider (Anthropic/OpenAI cachean por prefijo) y puede confundir
   al modelo a mitad de turno. Por eso los hosts lo tratan como configuración
   estática de sesión.
4. **Config leída una vez**. crush descubre `crushrc`/`crush.json` subiendo por
   el árbol y los carga al inicio; no los vigila. Igual con el contexto
   (`AGENTS.md`/`CRUSH.md`/`CLAUDE.md`).
5. **Skills son ficheros leídos al inicio**. `internal/skills/` indexa `SKILL.md`
   (frontmatter YAML) una vez; son fragmentos de prompt, no hay fs watch.
6. **Simplicidad y seguridad**. Recargar en caliente añade concurrencia (carrera
   con tool-calls en vuelo), manejo de errores y estados de UI. Los vendors
   eligieron el camino determinista y barato: reiniciar.
7. **Estado del servidor**. Un servidor MCP puede tener sesión propia (tokens
   OAuth, resources abiertos). Reemplazarlo en vivo no es trivial.

Contraste: **dsh** sí tiene la base para hacerlo bien. Cordis registra cada
plugin de forma **reversible** vía `ctx.effect()` y las tools viven en
`ctx.tools` (ver §3.3 del doc de referencia), así que un plugin de tools puede
darse de alta/baja sin reiniciar. CodeLoom hereda esa idea (`registro reversible`)
pero añade lo que falta: detección de cambios, aplicación en fronteras seguras y
observabilidad en la UI.

**Objetivo de este spec**: que CodeLoom cargue y descargue servidores MCP, tools y
skills **sin reiniciar**, de forma segura, observable y sin romper llamadas en
vuelo.

## 1. Objetivo / no-objetivos

**Objetivo**

- Añadir/quitar/reiniciar servidores MCP en caliente (config local y
  `list_changed` del servidor).
- Registrar/desregistrar sus tools y reflejarlas en la próxima petición.
- Reindexar skills y refrescar sus fragmentos de prompt en caliente.
- Conservar la **consistencia**: una tool-call en vuelo nunca pierde su tool.

**No-objetivos**

- No implementar MCP ni skills en m1/m2; esto es diseño para **m4** (§11).
- No hot-reload de `codeloom-core` ni del binario.
- No crecimiento del system prompt sin límite (ver §6.5).

## 2. Modelo: generaciones

El estado recargable se versiona. Tres piezas:

- **`ToolRegistry` versionado**: en vez de `HashMap` mutable compartido, un
  **snapshot inmutable** (`Arc<ToolRegistrySnapshot>`) con un `generation: u64`.
  Se publica con `ArcSwap`/`watch` (ya previstos en §3 del spec principal).
- **`SkillIndex` versionado**: mismo patrón; agrupa los `SKILL.md` descubiertos.
- **`McpSupervisor`**: una tarea tokio por servidor MCP + una tabla de estado.

Invariante central: **el step-loop captura el snapshot al inicio del step** y lo
usa para construir la petición y para resolver cada tool-call de ese step. Un
cambio publicado a mitad de step **no** afecta al step en curso; se aplica en el
siguiente. Esto da atomicidad sin locks en el camino caliente.

## 3. Seams afectados

| Seam | Hoy | Con hot-reload |
|---|---|---|
| `config/` | carga `codeloom.toml` + `AGENTS.md` al boot | watcher con debounce; publica config nueva |
| `tools/` (registro) | mapa estático por config | snapshots versionados + `register`/`unregister` |
| `mcp/` (nuevo subseam) | no existe | `McpSupervisor`: alta/baja/reconexión por servidor |
| `skills/` (nuevo) | no existe | `SkillIndex` + watcher de directorios |
| `event/` | bus durable + live | nuevos eventos de recarga (§7) |
| `agent/` (step-loop) | build request de un registro fijo | captura snapshot + `generation` por step |

## 4. Semántica de aplicación

1. Un cambio (config, MCP `list_changed`, skill) se **detecta** (§5) y entra a
   una cola de reconciliación.
2. El reconciliador calcula un **snapshot nuevo** (diff contra el actual) y lo
   **publica** con `ArcSwap`. La generación se incrementa.
3. El step-loop, al empezar cada step, lee el snapshot vigente. Los tool-calls
   de ese step se resuelven **contra ese snapshot**.
4. Si un cambio llega a mitad de step, se aplica en el step siguiente. Nunca se
   interrumpe un stream ni se mutate el registro bajo los pies de una call en
   vuelo.

Consecuencia buscada: recargar es **eventual y seguro**, no instantáneo a mitad
de turno.

## 5. Detección de cambios

### 5.1 MCP: notificaciones nativas (preferida)

CodeLoom es **cliente** MCP. Tras `initialize`, se suscribe a las notificaciones de
lista del servidor y, al recibirlas, marca ese servidor como "sucio" y re-consulta
`tools/list` (y `resources/list`/`prompts/list` si aplica). Esto cubre servidores
que cambian sus tools sin tocar disco (p. ej. un gateway que carga plugins).

> Verificar nombres y forma exactos contra la versión de la spec MCP soportada
> antes de implementar; el conjunto de notificaciones varía entre revisiones.

### 5.2 Config local: watcher de ficheros

Para alta/baja de **servidores** y cambios de `codeloom.toml`/skills:

- Watcher con `notify` sobre: `codeloom.toml`, el fichero de definición de MCP, el
  directorio de skills del workspace y el de usuario.
- **Debounce** obligatorio (~200-300 ms): los editores escriben varias veces
  (write+rename); sin debounce se disparan recargas espurias.
- Un fichero ilegible o JSON inválido **no** reemplaza el estado vigente
  (ver §9).

### 5.3 Disparo manual

- Comando de usuario en la TUI (`/reload`): fuerza reconciliación de MCP + skills.
- Señal `SIGHUP` (Unix) para recarga completa, útil en scripting.

## 6. Consistencia y concurrencia

### 6.1 Snapshot + generación

`ToolRegistrySnapshot { generation, tools: HashMap<Name, Arc<dyn Tool>> }`,
inmutable y `Send + Sync`. Lectura sin lock por `ArcSwap`. El step-loop guarda
la generación con la que construyó la petición para poder auditar/registrar
(§7) y para resolver resultados tardíos.

### 6.2 Tool-calls en vuelo

- **No se elimina una tool mientras tiene una call en vuelo.** El unregister
  marca la tool como *retiring*; se retira de los snapshots nuevos (no se
  ofrece al modelo) pero sigue resolviendo la call que ya arrancó, hasta su
  `ToolResult`.
- Un servidor MCP con calls en vuelo se **drena** antes de matarlo (kill
  cooperativo + timeout; pasado el timeout, se marca la call como
  `ToolResult { is_error }`).

### 6.3 No auto-aprobación

Una tool nueva **nunca** hereda permisos ni entra en `--yolo` por el hecho de
recargarse. Arranca en el flujo normal de permisos (§5.4 del spec principal).
Esto evita que un fichero de config aparecido en caliente se convierta en una
puerta de ejecución.

### 6.4 Prompt cache

Cambiar el conjunto de tools altera el prefijo de la petición e invalida el
prompt cache del provider para esa vuelta. Es un coste **aceptado y acotado**:
se paga una vez por recarga efectiva, en una frontera de step. Mitigaciones:

- Agrupar cambios y publicar **un** snapshot por ventana de debounce.
- Ordenar las tools de forma **estable** (por nombre) para que altas/bajas no
  reordenen el resto del bloque y maximizar el prefijo cacheable.

### 6.5 Crecimiento del contexto

Cada tool añade su esquema y cada skill su fragmento. Se aplican límites:

- Presupuesto de tokens para el bloque de tools/skills; al exceder, se prioriza
  y se avisa en la UI (no se trunca en silencio).
- Un servidor con MCP "verboso" puede degradar el prompt; el límite por defecto
  es parte de `codeloom.toml` (§8).

## 7. Eventos y UI

- **Durable** (replay): `tool/registry-changed { generation, added, removed }`,
  `skill/index-changed`, `mcp/server-state`.
- **Live** (UI): estado por servidor y contadores del flock de tools/skills.
- **Header**: indicador de recarga (`↻ 3 MCP · 12 tools`); en fallo, `⚠` con el
  motivo, **manteniendo** el estado anterior.
- Cada tool nueva aparece en el transcript con su origen (`mcp:<servidor>` /
  `skill:<nombre>`) para trazabilidad.

El bus ya separa durables de live (§4.4 del spec principal); no hace falta
carril nuevo.

## 8. Config (`codeloom.toml`)

Extensión de config, coherente con §5.2 del spec principal:

```toml
[hot_reload]
enabled = true
debounce_ms = 250
watch = ["codeloom.toml", "skills/", "~/.config/codeloom/skills/"]
mcp_notifications = true      # consumir list_changed del servidor

[tools]
max_schema_tokens = 8000      # presupuesto del bloque de tools

[mcp.servers.<id>]            # definición; alta/baja en caliente
transport = "stdio"           # stdio | sse | http
command = ["npx", "-y", "..."]
watch_notifications = true
```

## 9. Errores y degradación

| Caso | Comportamiento |
|---|---|
| Servidor MCP no arranca | estado `Failed`; se mantiene el snapshot previo; reintento con backoff + jitter |
| Servidor se cae en caliente | `Disconnected`; tools asociadas se retiran de snapshots nuevos; calls en vuelo -> `is_error`; reconexión con backoff |
| `codeloom.toml` inválido tras editar | se rechaza el cambio, se conserva el estado vigente, `⚠` en UI |
| Skill con `SKILL.md` roto | se ignora esa skill; el resto del índice se conserva |
| `list_changed` con tools desconocidas | se re-consulta `tools/list` y se reconcilia; conflicto de nombre -> gana la existente y se avisa |
| Timeout de drain de un servidor | kill forzado; calls pendientes -> `is_error` |

Principio: **el estado vigente solo se reemplaza por uno válido**. Recargar
nunca deja el agente sin tools.

## 10. Testing

| Nivel | Qué cubre | Herramientas |
|---|---|---|
| Unit | diff de snapshots (gen N -> N+1), retiring de tools, debounce, parseo de config | `cargo test` |
| Integración | supervisor MCP contra servidor falso que emite `list_changed`; drain de calls en vuelo | `tokio::test`, mock |
| Integración | watcher: editar config -> nueva generación aplicada al siguiente step | `tokio::test`, `tempfile` |
| Property | nunca se elimina una tool con call en vuelo; generaciones monótonas | `proptest` |
| Snapshot UI | header con recarga en curso / fallida | `TestBackend` + `insta` |

Sin red real: servidores MCP y providers siempre mock.

## 11. Encaje con milestones

- **m4** del spec principal (§9.2) pasa de "MCP, LSP, hooks, skills,
  persistencia" a incluir explícitamente **hot-reload**, y este documento se
  convierte en su spec de referencia.
- Depende de seams ya dispuestos: bus durable/live (§4.4), `ArcSwap`/`watch`
  (§3.5) y el registro de tools (§5.2). No requiere cambiar el modelo de
  dominios del step-loop; solo **versionar** el registro y capturar el snapshot
  por step.

## 12. Alternativas consideradas

- **A (recomendada)** — snapshots versionados + supervisor MCP + watcher con
  debounce, aplicado en frontera de step. Granular, seguro, observable.
- **B — reinicio del subsistema de tools**: matar todos los MCP y reconstruir el
  registro ante cualquier cambio. Más simple, pero descarta calls en vuelo y no
  distingue qué cambió; peor UX.
- **C — proxies perezosos por tool**: registrar un proxy siempre y resolver el
  provider en la llamada. Enmascara la recarga pero deja tools "fantasma" en el
  prompt y complica permisos. Descartada.
