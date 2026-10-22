# Cómo gestionan el contexto otras herramientas

Análisis de solo lectura del 2026-10-22 sobre el código de los repos de referencia. Cabe señalar
que las rutas de fichero y las cifras las leyeron agentes de exploración; las líneas exactas pueden
variar ligeramente, los valores se vieron en el código. «No consta» = no se encontró en ese repo.
Repos: `codex`, `claude-code-rust`, `pawan`, `opencode`, `qwen-code`, `martty` (en
`G:\loom-references\`, véase `REGISTRY.md`), `crush`, `deepseek-harness`, `mastra`, `hermes-agent`
y `herdr` (en `E:\workspace\personal\`, solo lectura). `hermes-agent` solo es un contenedor de la
imagen upstream, `herdr` es un multiplexor de terminal, `martty` es un cliente ACP y
`claude-code-rust` delega el motor en el SDK: ninguno aporta datos de gestión de contexto propios.

## Tope de la salida de una herramienta

| Herramienta | Tope | Qué se conserva | Guarda el original en disco |
|---|---|---|---|
| codex | 10 000 tokens (catálogo) o 10 000 bytes (por defecto); captura de 1 MiB | cabeza y cola 50/50, con el número exacto omitido | no |
| crush (bash) | 30 000 de ancho de celda | cabeza y cola 50/50 | **sí**: retención 7 días, 256 MiB por directorio, ruta en el marcador |
| opencode | 2 000 líneas y 50 KiB; configurable | cabeza (shell: cola) | **sí**: 7 días, ruta en el marcador |
| qwen-code | 25 000 caracteres y 1 000 líneas; 30 000 en shell; 200 000 por lote de llamadas | por defecto 1/5 cabeza y 4/5 cola | **sí**: 0600, 50 MB por fichero, 500 MB por sesión, ruta y sha256 |
| deepseek-harness | 12 500 tokens genéricos; shell 64 000 bytes por flujo | cabeza y cola; shell, cola | **sí**: hasta 64 MiB, ruta en el aviso; `read` exento para evitar bucles |
| mastra | 2 000 tokens, 200 líneas | cola (modo «sándwich»: 10 % cabeza) | no |
| pawan | 8 000 caracteres; bash 50 000 bytes | cabeza | no (y corta por bytes sin respetar UTF-8) |
| **Codeloom** | 20 000 bytes (≈ 5 000 tokens) | solo cabeza | **no** |

## Listados y búsquedas

| Herramienta | glob | grep | Respeta `.gitignore` |
|---|---|---|---|
| crush | 100 | 100 y 500 caracteres por línea | sí |
| opencode | 100 | 100 | sí |
| qwen-code | 100 (recoge 1 000) | 20 000 caracteres | sí |
| deepseek-harness | 100 | 250 y 2 000 bytes por línea | no consta |
| pawan | 50 | 100 líneas | no consta |
| **Codeloom** | 300 | 200 y 300 caracteres por línea | sí |

## Compactación del historial

- **Con llamada extra al modelo:** codex (90 % de la ventana; conserva hasta 20 000 tokens de
  mensajes de usuario), crush (cuando queda el 20 % de la ventana, o 20 000 tokens en ventanas de
  más de 200 000), opencode (techo = ventana − 15 %, mínimo 16 000; conserva 15 000 tokens),
  qwen-code (85 %; restaura hasta 5 ficheros), deepseek-harness (80 %; plantilla de 8 secciones),
  mastra (memoria observacional).
- **Sin modelo:** pawan (poda por importancia sobre tokens estimados bytes/4).
- **Poda de resultados antiguos sin resumir** (lo que se planea aquí): **qwen-code** (limpia
  resultados de `read_file`, shell, grep, glob… con el marcador `[Old tool result content cleared]`,
  conserva los 5 más recientes, se dispara por inactividad de 60 minutos o por 500 000 caracteres y
  limpia hasta la mitad del umbral) y **deepseek-harness** (poda determinista previa al resumen:
  por encima de 8 192 caracteres deja 4 096 al principio y 1 024 al final; no destructiva, el
  registro original queda intacto). codex, crush y opencode: no consta.
- Comunes: estimar tokens como bytes/4 (o caracteres/4); anclar en el conteo real del proveedor y
  estimar solo lo nuevo (opencode, qwen-code); recuperarse de un rechazo real del proveedor por
  contexto excedido (deepseek-harness, opencode, qwen-code).

## Presupuesto de tokens o coste

- **qwen-code:** `sessionTokenLimit` (por defecto ilimitado); al superarlo termina el turno con un
  mensaje claro. También presupuesto por turno por directiva del usuario.
- **mastra:** `TokenCostControlProcessor`, con ámbito y ventana; modo `block` o `warn` y aviso
  suave a un porcentaje.
- **codex:** aviso al modelo cuando quedan 6 144 tokens (función desactivada por defecto).
- **pawan:** aviso al modelo con 3 iteraciones restantes.
- **opencode:** al alcanzar el máximo de pasos, ordena responder solo con texto.
- crush y deepseek-harness: no consta un tope de coste.

## Resultado de un subagente hacia el padre

qwen-code: 32 000 caracteres, conservando la **cola** (donde está la conclusión), con volcado a
disco. codex: 1 000 tokens solo para el texto de error. opencode y deepseek-harness: el tope
genérico de resultados de herramienta. crush, mastra, pawan: sin tope propio.

## Ficheros de contexto del proyecto (`AGENTS.md`…)

codex 32 KiB en total (corta la cola y solo lo registra, sin marcador en el texto); deepseek-harness
64 KiB; mastra 1 000 tokens por recordatorio. crush, opencode, pawan, qwen-code: sin tope.
**Codeloom: sin tope.**

## Lo que aprendemos (lectura propia)

1. **Cuatro de los cinco agentes con bucle propio guardan la salida completa en disco y ponen la
   ruta en el marcador** (crush, opencode, qwen-code, deepseek-harness). Lo recortado no se pierde:
   el modelo puede volver a leerlo con `read` o `grep`. Codeloom hoy lo pierde.
2. Conservar **cabeza y cola** es lo habitual; solo cabeza (lo nuestro) pierde la conclusión de un
   comando largo. Para resultados de subagente, qwen-code conserva la cola.
3. La poda de resultados antiguos sin resumir tiene dos precedentes (qwen-code, deepseek-harness);
   ambos conservan los más recientes y dejan un marcador.
4. Sin tope de tamaño en `AGENTS.md` estamos en el grupo mayoritario, pero hay precedentes de 32-64
   KiB; si se pone, con marcador visible (codex lo calla).
5. El presupuesto de tokens es raro y siempre tiene una política explícita (terminar el turno,
   bloquear o avisar).
