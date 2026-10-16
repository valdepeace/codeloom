# Bucle de entrada de loom: sin bloqueos, sin lag, sin parpadeos

> Estado: **diseño propuesto**, pendiente de revisión.
>
> Fecha: 2026-10-16
>
> Prioridad: el composer y la entrada son el eje de la app (ver memoria
> "composer-smoothness-priority"). Referencias: codex-rs `tui/src/tui.rs`
> (`FrameRequester`, `TuiEvent::{Draw, Paste, Resize}`, `sync_update`,
> `EnableBracketedPaste`), Crush (cola de prompts), awesome-ratatui (consulta
> si ratatui no lo cubre: `terminput`, `rat-salsa`).

## 0. Problemas del bucle actual (`codeloom-tui/src/run.rs`)

1. **Una tecla por frame**: bus → `draw` → `poll(50 ms)` → una tecla. Teclear
   rápido o pegar cuesta un `draw` por carácter.
2. **Pegar envía mensajes**: sin *bracketed paste*, cada `\n` pegado llega como
   `Enter` y manda trozos al agente.
3. **Bloquea tokio**: `event::poll` es bloqueante dentro de un `async fn`.
4. **Redibuja sin cambios** cada 50 ms.
5. **Pierde prompts**: `try_send` descarta si el canal (8) está lleno.
6. **Sin sincronización de frame**: en terminales lentos un frame grande puede
   verse a medias.

## 1. Objetivos medibles

- Ninguna tecla espera a más de **un** `draw` (las ráfagas se agrupan).
- Pegar N caracteres = **un** evento y **un** `draw`; nunca envía.
- Nada bloqueante en el runtime; el bucle solo espera en `select!`.
- Sin cambios no se pinta; con streaming, máximo ~60 fps.
- Ningún prompt se pierde.

## 2. Diseño

### 2.1 Entrada asíncrona

`crossterm` con features `event-stream` y `bracketed-paste`;
`EventStream` como `Stream<Item = io::Result<Event>>`. `RawGuard` activa
también `EnableBracketedPaste` (y lo desactiva al salir).

### 2.2 Bucle por eventos

```text
loop {
  select! {
    biased;
    ev = input.next()      => { handle(ev); drain_ready(input); input_dirty }
    ev = bus_rx.recv()     => { apply(ev); drain(bus_rx); bg_dirty }
    req = perm_rx.recv()   => { enqueue_permission(req); input_dirty }
    _ = sleep_until(deadline), if frames.pending() => {}
    _ = anim_tick.tick(), if animating => { advance(); bg_dirty }
  }
  if frames.due(now) { draw_synchronized(); frames.drawn(now) }
}
```

- `biased`: la entrada tiene prioridad sobre el bus.
- **Agrupar ráfagas**: tras un evento de teclado se consumen sin esperar
  (`now_or_never`) los que ya estén listos, hasta un tope (512), y se pinta una
  vez.
- **Bus**: se drena con `try_recv` antes de pintar.

### 2.3 Planificador de frames (`FrameScheduler`, puro y testeable)

```rust
pub enum Urgency { Input, Background }
pub struct FrameScheduler { pending: Option<Urgency>, last: Option<Instant>, min_interval: Duration }
impl FrameScheduler {
    pub fn request(&mut self, u: Urgency);
    pub fn due(&self, now: Instant) -> bool;      // Input: siempre; Background: si pasó min_interval
    pub fn deadline(&self) -> Option<Instant>;     // cuándo vence un Background pendiente
    pub fn drawn(&mut self, now: Instant);
}
```

- `Input` (teclas, pegado, resize, diálogo): se pinta **ya**.
- `Background` (deltas del agente, animaciones): como mucho cada 16 ms.
- Sin peticiones no se pinta.

### 2.4 Pegado

`Event::Paste(s)`: se normaliza (`\r\n` y `\r` → `\n`) y se inserta con
`Composer::insert_str`; resetea el historial. **Nunca envía.** Con el diálogo
de permisos abierto o el foco en el transcript, se ignora.

### 2.5 Frame sincronizado

Cada `draw` va entre `BeginSynchronizedUpdate`/`EndSynchronizedUpdate`
(DEC 2026): el terminal muestra el frame completo de una vez. Terminales sin
soporte lo ignoran.

### 2.6 Animaciones

`anim_tick` (50 ms) solo está activo si hay algo animándose: efecto `fx` en
curso o tool con spinner. El efecto avanza con el tiempo real transcurrido, no
con 50 ms fijos.

### 2.7 Prompts sin pérdida

El canal de prompts pasa a `unbounded`. Si el agente está ocupado, el prompt
espera en el canal; la cabecera muestra `en cola (N)` (contador en `App`,
decrementado al recibir `TurnStarted`).

### 2.8 Estructura

- `run.rs` queda como orquestación de IO (terminal, streams, `select!`).
- La lógica pura (`handle_key`, `handle_paste`, `FrameScheduler`) en módulos
  testeables sin terminal.

## 3. Tests

- `FrameScheduler`: `Input` vence ya; `Background` respeta 16 ms; sin peticiones
  no vence; `deadline` correcto; `Input` tras `Background` adelanta el frame.
- Pegado: texto con `\n` y `\r\n` queda en el composer sin enviar; ignorado con
  diálogo abierto.
- Ráfaga: 2 000 eventos de teclado procesados con una sola petición de frame
  (contando `request` + `due`).
- Cola de prompts: enviar dos con el agente ocupado → `en cola (2)`; baja con
  `TurnStarted`.
- Integración (sin terminal real): el bucle con un `Stream` de eventos simulado
  y un backend de test no pinta sin cambios y pinta una vez por ráfaga.

## 4. No-objetivos

- Ratón (va con la selección por ratón).
- Medición de latencia real en terminal (se valida a mano).
- Cambios en batten salvo los que pida `insert_str` (ya existe).

## 5. Preguntas abiertas

- ¿Tope de agrupado (512) y frame mínimo (16 ms) configurables en
  `codeloom.toml`? Propuesta: constantes; configurables si hace falta.
