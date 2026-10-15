# CodeLoom — Idea: resumen del último turno (1-3 frases)

> Estado: **idea en exploración**. No implementar.
>
> Fecha: 2026-10-14
>
> Relacionado: `docs/specs/2026-10-13-loom-design.md` (§4.3 turn/step, §6.4
> avisos, §7 UI, §5.5 extensiones) y
> `docs/specs/2026-10-14-loom-voice-prompt-input.md` (capa de voz).
> Proyecto externo de referencia: `tipsy-assistant`.

## 0. La idea

Al cerrar **el último turno**, producir un **resumen de 1 a 3 frases como
máximo** del intercambio, que sirva a dos consumidores a la vez:

- **Texto**: un "tl;dr" compacto del último turno en el transcript de CodeLoom.
- **Voz**: lo que tipsy lee en voz alta (el resto de la respuesta larga no se
  recita).

Una sola pieza resumida, dos salidas. Vive en la **frontera CodeLoom <-> tipsy**:
CodeLoom genera el resumen (tiene el transcript y el provider), tipsy lo habla.

## 1. Por qué tiene sentido

- Una respuesta técnica (con diffs, comandos, trazas) es **insufrible en audio**;
  oírla entera es ruido. Un resumen de 1-3 frases es lo que un humano diría.
- El transcript, tras muchos turnos, se vuelve ilegible; un tl;dr por turno da
  un índice navegable sin salir del hilo.
- tipsy ya sintetiza **frase a frase** (`brain`: divide en frases, sintetiza en
  paralelo y emite en orden). Cambiar "todas las frases" por "las del resumen"
  es un ajuste, no un motor nuevo.

## 2. Quién genera el resumen

| | A. CodeLoom genera (recomendada) | B. tipsy genera |
|---|---|---|
| Quién tiene el transcript | CodeLoom | no |
| Quién tiene provider LLM | CodeLoom | tipsy (chat provider) |
| Render en transcript | trivial (evento de CodeLoom) | requiere reenviar a CodeLoom |
| tipsy hace | solo TTS | LLM + TTS |

**A.** CodeLoom ya tiene el turno completo y su `trait Provider`. tipsy se queda
como **voz**: recibe el texto resumido y lo dice. tipsy ya expone el comando
`Say` ("no preguntes, recita `text`"), así que CodeLoom **envía el resumen con
`Say`** y no hay que tocar su protocolo.

## 3. Cuándo y cómo

- **Disparo**: solo cuando el turno **sienta** (después del último step, cuando
  no hay tool-calls). Nunca a mitad de turno ni por step.
- **Fuera del camino caliente**: el resumen es una tarea aparte que corre al
  cerrar el turno; **no retrasa** la respuesta real ni bloquea el loop. Llega un
  poco después y se añade al transcript.
- **Entrada del resumen**: el prompt del usuario + el último mensaje del
  assistant del turno (su `Text`, sin los `ToolResult`).
- **Modelo**: reutiliza el seam de providers. Por defecto un modelo **rápido y
  barato** configurable; puede ser el mismo del chat o uno distinto.
- **Límite duro 1-3**: se pide en el prompt **y** se recorta después partiendo
  por frases (`max_sentences`, def. 3). El recorte garantiza el tope aunque el
  modelo se pase.

## 4. Flujo

```
turn_end
  ├─ (respuesta real ya en el transcript) ────────────────► UI
  └─ tarea de resumen (async, provider)
        └─ texto <= 3 frases
             ├─ evento durable turn/summary ──► transcript ("resumen")
             └─ lo al consumidor de voz ──► tipsy  Say(summary) ──► audio
```

- Evento **durable** `turn/summary { turn_id, text }` (replay, aparece en el
  transcript) y evento **live** para pintarlo en cuanto llega.
- El resumen **no reemplaza** la respuesta; se muestra marcado (p. ej. "resumen"
  o estilo atenuado) para distinguirlo del contenido real.
- Si el resumen falla, el turno sigue siendo válido: se omite el tl;dr y no se
  habla nada extra.

## 5. Relación con la capa de voz

- Sin voz: el resumen es solo texto (tl;dr del turno).
- Con voz (`codeloom-voice`/tipsy): el **resumen es lo que se habla**, no la
  respuesta completa. La voz y el texto comparten el mismo resumen.
- Alternativa de tipsy: un modo "leer resumen, mostrar completo", donde tipsy
  divide en frases solo el resumen que CodeLoom le manda por `Say`.

## 6. Gotchas

- **No resumir cada step**: solo el turno cerrado. Resumir de más contamina el
  transcript y gasta tokens.
- **No bloquear el turno** por el resumen; es un extra best-effort.
- **Coste**: una llamada corta por turno; acotar con un tope de tokens de salida
  y opcionalmente saltar turnos triviales (respuesta ya de <= 3 frases).
- **No saltarse permisos**: el resumen es texto derivado, nunca ejecuta nada.
- **Idioma**: se resume en el idioma del perfil/usuario para que la voz suene
  natural; tipsy ya trabaja por idioma de perfil.

## 7. Config

```toml
[summary]
enabled = true
max_sentences = 3          # tope duro 1-3
model = "google/gemini-2.5-flash"   # rápido y barato; opcional
speak = true               # enviar el resumen a la voz (tipsy) si está activa
```

## 8. Encaje

- Extensión de §5.5 del spec principal (como MCP/skills y voz).
- Depende de: bus durable/live (§4.4), `trait Provider` (§4.2), turn/step
  (§4.3) y, para la voz, la idea `codeloom-voice`.
- tipsy no se modifica: basta su comando `Say`.
