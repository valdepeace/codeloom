# CodeLoom — Idea: línea de estado de turno (proveedor · modelo · tiempo)

> Estado: **idea en exploración**. No implementar.
>
> Fecha: 2026-10-15
>
> Relacionado: `docs/specs/2026-10-13-loom-design.md` (§4.3 turn/step, §7 UI),
> `docs/specs/2026-10-14-loom-turn-summary.md` (otra cosa: resumen LLM).
> Referencia de UX: el pie de Crush
> (`DeepSeek: DeepSeek V4.1 Flash via OpenRouter in 43s ──────`).

## 0. La idea

Al **cerrar cada turno**, añadir al transcript una **línea de estado** con quién
respondió y cuánto tardó:

```
DeepSeek: DeepSeek V4.1 Flash via OpenRouter in 43s ─────────────────────────
```

Es una línea de **metadatos**, no de contenido. Se distingue del resumen LLM de
1-3 frases (`loom-turn-summary.md`), que es texto generado; esto son hechos del
turno.

## 1. Datos y origen

| Campo | Origen |
|---|---|
| Proveedor / gateway ("via OpenRouter") | `ProviderKind` / `base_url` de la config |
| Modelo | `config.model` |
| Duración ("in 43s") | `TurnEnded` − `TurnStarted` en el agent loop |

Todo lo tiene ya el agent loop y la config; no hace falta nada nuevo salvo
transportarlo a la UI.

## 2. Encaje

- **Core**: emitir un evento al cerrar el turno, p. ej.
  `LiveEvent::TurnFinished { provider, model, elapsed_ms }` (o ampliar
  `TurnEnded`).
- **batten**: una línea de estilo `Banner`/`Muted` con una regla de guiones
  (`─`/`Glyphs::field`) hasta el ancho. Puede ser un widget `status_line` o un
  `LineKind` ya existente (`Banner`).
- **codeloom-tui**: `App::apply` la añade al transcript; el render ya soporta
  líneas con prefijo/estilo.

## 3. Gotchas

- **No confundir** con el resumen LLM (`loom-turn-summary.md`): coexisten, son
  capas distintas.
- Duración: contar **solo el turno** (no el resumen diferido).
- Ancho: la regla de guiones se recorta al ancho; a anchos mínimos, se omite la
  regla y queda solo el texto.

## 4. Preguntas abiertas

- ¿Se pinta **siempre** al cerrar cada turno, o solo bajo demanda/verbose?
- ¿Formato exacto (separador `:` y regla `─`) y en qué idioma?
- ¿Componente nuevo en batten (`status_line`) o reutilizar `Banner` con spans?
- ¿El tiempo se cuenta desde el prompt del usuario o desde el primer token?
