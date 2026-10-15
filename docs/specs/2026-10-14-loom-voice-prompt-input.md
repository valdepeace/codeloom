# CodeLoom — Idea: entrada por voz al prompt (dictado)

> Estado: **idea en exploración**. No implementar.
>
> Fecha: 2026-10-14
>
> Relacionado: `docs/specs/2026-10-13-loom-design.md` (§5.5 extensiones, §7 UI,
> §8 herdr). Proyecto externo de referencia: `tipsy-assistant`
> (`E:/workspace/personal/tipsy-assistant`).

## 0. La idea

Poder **dictar el prompt por voz** en vez de escribirlo: un asistente de voz que
"pinte" el texto en el composer de CodeLoom (o lo envíe) mientras CodeLoom está activo.
La pregunta original: ¿es posible?, ¿se hace como plugin de CodeLoom o como plugin
de herdr que escribe en el panel?

## 1. ¿Es posible? Sí, y el problema no es el que parece

Nada del diseño de CodeLoom lo impide. "Pintar en el prompt" separa en realidad
**tres** decisiones independientes:

1. **Quién hace STT** (voz -> texto). No es trivial: necesita captura de audio,
   VAD, y un provider de transcripción.
2. **Dónde entra el texto** (el *seam* de inyección): en el composer nativo de
   CodeLoom, o en el PTY del panel.
3. **Qué hace CodeLoom con el texto**: rellenar el borrador (el usuario revisa y
   pulsa Enter) o enviarlo directamente como prompt.

`tipsy-assistant` ya resuelve (1) por completo: es un asistente de voz en Rust,
**cliente-servidor**, con STT, VAD, wake word, proveedores (`openai_compat`,
`gemini`, `elevenlabs`) y un protocolo `Command`/`Event` sobre WebSocket (ver su
`docs/architecture.md`). CodeLoom **no debería reimplementar audio**; debería
consumirlo.

## 2. Topologías

| | A. CodeLoom cliente de un dictado | B. Plugin de herdr | C. tipsy como host |
|---|---|---|---|
| Quién hace STT | tipsy (WS) o un provider propio | el plugin (herdr) | tipsy |
| Dónde se inyecta | seam de entrada de CodeLoom | PTY del panel | n/a |
| Funciona con | solo CodeLoom | cualquier agente del panel | no aplica |
| Fragilidad | baja (canal propio) | alta (teclas/foco/UTF-8) | — |
| Veredicto | **recomendada** | posible, útil fuera de CodeLoom | descartada |

### A. CodeLoom como cliente de un servicio de dictado (recomendada)

CodeLoom añade un **canal de entrada externa** a su composer y, opcionalmente, un
cliente del servidor de tipsy. tipsy hace todo el audio; CodeLoom solo recibe texto.

- tipsy ya expone `ListenStart`/`ListenStop` (comandos) y emite `Heard`
  (evento con lo transcrito), además de `Reply`/`SpeechChunk`. CodeLoom se conecta a
  su WebSocket, abre el micrófono y consume `Heard`.
- Alternativa sin tipsy: CodeLoom usa su propio provider de transcripción. Más
  trabajo y fuera del objetivo; solo si se quiere evitar la dependencia.

### B. Plugin de herdr que escribe en el panel

herdr es quien posee el PTY, así que **puede** escribir en el panel de cualquier
agente, no solo en CodeLoom. El análisis de herdr cita métodos como `agent.prompt`,
`agent.wait` y `pane.report_agent` (ver §3.1 del doc de referencia), pensados
para *promptear* a un agente que corre en un panel.

Ventaja: sirve para todos los agentes. Inconvenientes: inyectar texto en un TUI
ajeno es frágil (foco, bracketed paste, secuencias de escape, UTF-8/IME) y
depende de la superficie estable de herdr que se confirme. Es **más un producto
de herdr que de CodeLoom**; como idea separada tiene sentido, como mecanismo de CodeLoom no.

### C. tipsy como host de CodeLoom

Invertir la relación (tipsy lanza CodeLoom) contradice "terminal first" de tipsy y
el modelo de CodeLoom (motor + TUI propios). Descartada.

## 3. Arquitectura recomendada (A)

```
  micrófono
     │ audio
     ▼
┌───────────────┐   Command (ListenStart/Stop)   ┌────────────────────────┐
│ tipsy-assistant│◀──────────────────────────────│  codeloom-voice (opcional) │
│ server (STT/VAD│   Event: Heard { text }        │  cliente WS + captura  │
│  wake word)    │───────────────────────────────▶│                        │
└───────────────┘                                 └───────────┬────────────┘
                                                              │ texto
                                                channel externo (mpsc)
                                                              ▼
                                                  ┌────────────────────────┐
                                                  │ Composer de CodeLoom        │
                                                  │ (borrador -> Enter)     │
                                                  └────────────────────────┘
```

Piezas:

- **`codeloom-voice`** (crate opcional, como `codeloom-herdr`): cliente del WS de tipsy
  y, si algún día hace falta, captura de audio nativa. **No** entra en
  `codeloom-core` (mismo principio que herdr: el core no conoce el host).
- **Seam de entrada externa** en CodeLoom: hoy el composer manda `input -> motor` por
  `mpsc` (§7 del spec principal). Se añade un **canal de entrada** para fuentes
  externas (voz, IPC, script) que escribe en el mismo buffer del composer. La
  inyección es un evento de UI, no una tool: el texto es del usuario, no del
  modelo.
- **tipsy como backend de dictado**: reutiliza su servidor tal cual; CodeLoom es un
  cliente más, igual que su cliente de terminal o el web. Si tipsy no está
  corriendo, la función simplemente está apagada.

## 4. Borrador vs envío (la decisión de seguridad)

Un agente de código puede ejecutar `bash`. Un prompt mal transcrito
("borra" por "mueve") no debe ejecutarse sin más. Por defecto:

- La voz **rellena el borrador** del composer; el usuario revisa y pulsa `Enter`.
- El **auto-envío** es una opción explícita por configuración, y aun activada
  pasa por el mismo gate de permisos (§5.4 del spec principal): dictar no salta
  aprobaciones.
- Feedback visible de que el micrófono está abierto (indicador en el header), y
  marcar visualmente el texto inyectado por voz para distinguirlo de lo tecleado.

## 5. Gotchas

- **No reimplementar audio en CodeLoom.** tipsy ya tiene STT/VAD/wake word; duplicar
  eso es puro coste.
- **Wake word local.** En tipsy la detección corre en la máquina del usuario y
  nada sale a la nube hasta detectar la palabra clave; CodeLoom debe conservar esa
  propiedad.
- **Inyección por PTY es frágil** (opción B): si algún día se hace, es un
  producto de herdr, no de CodeLoom.
- **El prompt cache no se ve afectado**: la entrada es local; el texto dictado
  simplemente forma parte del turno que se envía.
- **La transcripción es del idioma del perfil** de tipsy (es/en y otros). CodeLoom no
  necesita saber de idiomas: recibe texto.

## 6. Seguridad y privacidad

- Audio solo a tipsy (local o proveedor elegido por el usuario); CodeLoom no persiste
  audio.
- Micrófono siempre con indicador visible; parar con una tecla.
- El texto dictado se trata como **input del usuario**: no cambia permisos ni
  allowlists.

## 7. Dónde vive qué

| Pieza | Repo |
|---|---|
| STT, VAD, wake word, proveedores de voz | `tipsy-assistant` |
| Cliente de dictado + seam de entrada + indicador UI | `codeloom` (`codeloom-voice` + TUI) |
| Inyección en paneles de terceros (opcional, ajeno a CodeLoom) | plugin de `herdr` |

Regla de oro: **tipsy = oído y boca; CodeLoom = cerebro y manos.** Se comunican por
el protocolo que tipsy ya tiene.

## 8. Preguntas abiertas / a verificar

- ¿El `Heard` de tipsy llega por fragmentos o como utterance cerrada? Afecta a
  cuándo se vuelca texto al composer.
- ¿CodeLoom consume tipsy por su WebSocket público (con token) o por un canal local?
- ¿Hace falta STT propio en CodeLoom, o tipsy como dependencia opcional basta?
- (Opción B) ¿herdr expone hoy inyección de texto en un panel, o solo
  `agent.prompt`? Confirmar contra su API antes de considerar esa vía.
