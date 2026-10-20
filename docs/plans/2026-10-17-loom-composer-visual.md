# Loom: composer multilínea visible — tarea local

> Tarea acotada para ejecutar con Crush y Qwen 3.5 4B en local.
>
> Contrato aprobado: `docs/specs/2026-10-15-batten-composer-editing-design.md`.
> Integración Batten/showcase: `docs/plans/2026-10-15-showcase-composer-contract.md`.

## Hallazgo

El estado y el widget del composer ya admiten edición multilínea y cursor real.
Loom reservaba una altura fija según el ancho del terminal, así que `Ctrl+J` o
pegar varias líneas podía recortar el texto y dejar el cursor fuera de vista.

## Objetivo

Hacer que el composer visible de Loom crezca con sus líneas lógicas, conservando
al menos una fila para el transcript cuando el tamaño del terminal lo permita.
Mantener intacto el contrato existente: `Enter` envía, `Ctrl+J` inserta línea,
el ancho no hace wrap y Batten sigue sin depender de crossterm.

## Alcance

- Revisar primero `crates/codeloom-tui/src/render.rs` y el render del widget en
  `crates/batten/src/widgets/composer.rs`.
- Ajustar el layout de Loom para que el alto del composer corresponda al texto
  multilínea, limitado por el alto disponible y dejando espacio al transcript.
- Si el alto disponible no permite mostrar todas las líneas, mantener el cursor
  visible y evitar pánicos en terminales pequeños.
- Añadir pruebas `TestBackend` del layout/render: una línea, varias líneas y
  terminal pequeño; comprobar el contenido visible y posición del cursor.
- Cambiar solo `crates/codeloom-tui/src/render.rs` y, si hace falta para
  mantener visible el cursor, `crates/batten/src/widgets/composer.rs`.

## Estado

- [x] El alto crece según las líneas lógicas y deja una fila para el transcript.
- [x] Si el terminal limita el alto, el widget desplaza el texto hasta el cursor.
- [x] `TestBackend` cubre composer multilínea y terminal pequeño; pasan las
  suites de `codeloom-tui` y `batten`.
- [x] Confirmar `Ctrl+J` y cursor en un terminal interactivo real (confirmado por el usuario el 2026-10-20).

## Límites

- No cambiar `Composer`, historial, atajos, especificación de wrap horizontal,
  componentes visuales de Batten ni el showcase.
- No añadir dependencias, configuración ni nuevos conceptos de estado.
- No actualizar ni crear commits; no usar red ni instalar dependencias.
- No ampliar el composer indefinidamente a costa de ocultar todo el transcript.

## Comprobación

Ejecutar desde Git Bash en Windows los checks de la tarea propietaria:

```bash
cargo test -p batten
cargo test -p codeloom-tui
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
```

Además, informar si se pudo abrir Loom en un terminal real para confirmar que
`Ctrl+J` deja visibles las líneas y el cursor. Si no se pudo hacer esa
comprobación, dejarla explícitamente pendiente; los tests de `TestBackend` no
demuestran el comportamiento visual del terminal real.

## Entrega

Informar archivos cambiados, regla de dimensionado aplicada, escenarios
comprobados y resultados. Si el terminal es demasiado pequeño para mostrar
composer y transcript a la vez, describir el límite observado.
