# Catálogo de proveedores y modelos desde models.dev

Estado: propuesta del 2026-10-21 (petición del usuario: «para los provider
podríamos usar models.dev para hacer un listado de provider y modelos, y tenemos
todos los datos habidos y por haber, y así no tenemos que hacerlo a mano»).
Plan: `docs/plans/2026-10-21-codeloom-models-catalog.md`.

## Intención

Hoy `/settings` obliga a teclear a mano el tipo de proveedor, la URL base, el
nombre de la variable de la clave y el identificador del modelo. `https://models.dev/api.json`
publica todo eso (verificado el 2026-10-21: 232 proveedores; por proveedor `id`,
`name`, `env` —variables de la clave—, `api` —URL base—, `npm` —tipo de SDK— y
`models`; por modelo `tool_call`, `reasoning`, `limit.context`, `limit.output`,
`cost.input/output`, `status`). Con ello se elige de una lista y se rellena todo.

## Objetivos

1. Un catálogo en memoria (`Catalog`) construido a partir de ese JSON, con solo
   lo que CodeLoom sabe usar: proveedores cuyo `npm` se traduce a un
   `ProviderKind` soportado y que tengan URL base; modelos con `tool_call` y sin
   `status` «deprecated».
2. Caché en disco (`~/.codeloom/models-dev.json`) con caducidad de 24 horas y
   refresco en segundo plano al arrancar, sin bloquear nunca la interfaz.
3. En `/settings`, un selector con filtrado por texto para proveedor y modelo.
   Elegir uno rellena tipo, URL, variable de clave y modelo.
4. Seguir permitiendo la edición manual (proveedores propios, modelos nuevos que
   aún no estén en el catálogo) y funcionar sin red.

## No objetivos

- Soportar tipos de SDK que `ProviderKind` no tiene (Azure, Google, Groq
  nativos, Bedrock…). Quedan fuera hasta que existan; los que exponen
  `@ai-sdk/openai-compatible` entran por la vía OpenAI-compatible.
- Guardar claves de API: solo se rellena el *nombre* de la variable.
- Mostrar costes en el rail (el dato queda disponible; es una mejora posterior
  natural para la observabilidad).

## Anti-objetivos

- Red en el camino de teclado o de dibujado, o en los tests (el catálogo se
  construye con una función de obtención inyectable; los tests usan un JSON de
  ejemplo).
- Que `codeloom-core` adquiera dependencias de interfaz.
- Que un fallo de red, un JSON corrupto o un catálogo vacío impidan arrancar o
  abrir `/settings`.

## Decisiones de diseño (mías, marcadas para revisión)

- **Traducción de `npm` a `ProviderKind`:** `@ai-sdk/anthropic` → `Anthropic`;
  `@ai-sdk/openai-compatible`, `@ai-sdk/openai` y `@openrouter/ai-sdk-provider`
  → `OpenAiCompat`. Los demás se descartan. Si falta `api` y el proveedor es
  `openai`, se usa `https://api.openai.com/v1`; si es `anthropic`,
  `https://api.anthropic.com`; el resto sin `api` se descarta.
- **Dónde vive:** módulo `catalog` en `codeloom-core` (parseo con `serde`, caché
  y obtención con `reqwest`, que ya es dependencia). La interfaz solo recibe un
  `Arc<Catalog>` ya construido.
- **Carga:** al arrancar se lee la caché (si existe) y se entrega ya; si falta o
  tiene más de 24 h, una tarea en segundo plano la descarga (tiempo límite 15 s),
  la escribe de forma atómica (fichero temporal + renombrado) y entrega el
  catálogo nuevo. Sin red y sin caché, el catálogo queda vacío y `/settings`
  funciona como hoy.
- **Interfaz:** el campo «Provider» pasa a abrir un selector (escribir filtra,
  ↑/↓ elige, Enter confirma, Esc cancela); lo mismo el campo «Model», acotado al
  proveedor elegido y mostrando contexto y precio de entrada/salida por millón.
  Los campos de texto actuales se conservan para la edición manual.
- Orden de los proveedores: los de uso frecuente primero (OpenRouter,
  Anthropic, OpenAI), luego alfabético; los modelos, por `release_date`
  descendente.

## Qué se deja al implementador

Estructura interna de los tipos del catálogo, el formato exacto de las filas del
selector y la distribución de teclas dentro del diálogo, mientras se cumpla lo
anterior.

## Verificación

Pruebas puras con un JSON de ejemplo (traducción de `npm`, filtros, orden,
tolerancia a campos ausentes, JSON corrupto), de la caché (caducidad, escritura
atómica, lectura de un fichero corrupto) y del selector (filtrado, elección que
rellena los campos, edición manual intacta). Sin red en los tests. Validación
visual en terminal real: pendiente.

## Estado (2026-10-21)

Implementado, cada tarea en su commit: marca de cambios sin guardar en
`/settings` (`ff79a17`), módulo `catalog` del núcleo (`0354aba`), carga en
segundo plano (`2f06275`) y selectores de proveedor y modelo (commit de la
tarea 4). Con el JSON real del 2026-10-21 el catálogo queda en 204 proveedores y
5667 modelos útiles.

Cómo se usa: en `/settings`, `Ctrl+L` (o `Enter` sobre «Provider») abre la lista
del campo enfocado; escribir filtra, `↑`/`↓` mueven, `Enter` elige y `Esc` cierra
solo la lista. Elegir proveedor rellena tipo, URL y variable de la clave; elegir
modelo rellena el modelo, mostrando `ctx 200k · $3/$15`. Lo elegido cuenta como
cambio sin guardar y la edición a mano sigue disponible.

Decisiones tomadas al implementar:

- `Enter` sobre «Provider» abre el selector; el tipo se sigue cambiando con `←`/`→`.
- El selector se cierra al elegir; no pasa solo a la lista de modelos.
- Si el proveedor del catálogo no trae variable de clave, se conserva la actual.
- La ruta de la caché usa `USERPROFILE`/`HOME`, sin añadir `dirs` al núcleo.
- Si falla la escritura de la caché, se avisa en el registro y el catálogo se
  entrega igualmente.

Limitaciones conocidas:

- Pegar texto no alimenta el filtro del selector.
- El selector del modelo exige que la URL actual coincida con un proveedor del
  catálogo; si no, avisa y hay que escribirlo a mano.
- El brazo del bucle que recibe el catálogo no tiene prueba automática (necesita
  terminal real); el canal y su aplicación sí.
- Los costes ya están en el catálogo, pero aún no se muestran en el rail.
- Validación visual en terminal real: pendiente.
