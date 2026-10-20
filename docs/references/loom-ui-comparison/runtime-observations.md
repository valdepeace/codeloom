# Capturas de ejecución: MastraCode y Crush

> Fecha: 2026-10-18. Registro textual de frames VT observados en PTY real; se
> quitaron secuencias ANSI para facilitar lectura. No son PNG ni conservan
> color/fuente. El tamaño del PTY no quedó fijado al mismo valor en ambas
> sesiones, por lo que no comparar alineación por columna.

## MastraCode 0.47.1 — composer vacío

- Binario: `node.exe` 24.21 de Codex + `mastracode/dist/cli.js`.
- CWD: `%TEMP%\loom-mastracode-visual-capture`.
- Base de datos aislada con `MASTRA_DB_PATH`; `MASTRACODE_DISABLE_MCP=1`;
  tema forzado a dark; proveedor visible `llama-server/local`.
- No se enviaron comandos ni prompts en la primera vista.

```text
Mastra Code v0.47.1
Project: loom-mastracode-visual-capture
Resource ID: loom-mastracode-visual-capture-bb2f881bcce5

shift+tab  cycle modes ·  /help  info & shortcuts

→ [cursor]

build · llama-server/local · 0/70k 0%
C:\Users\negrero\AppData\Local\Temp\loom-mastracode-visual-capture
```

`/help` mostró una lista de comandos agrupada visualmente con atajos. Incluye
sesiones, modelos/modos, goals, permisos, memoria, herramientas y plugins,
diagnóstico, tema, integraciones y salida. El pie del composer mantiene el
modo/modelo y el uso de contexto.

## MastraCode — llamada de solo lectura

Fixture temporal `README.md` (135 bytes). Se pidió abrirlo con una herramienta
de lectura y resumirlo, explícitamente sin editar ni ejecutar comandos. El
proveedor de la sesión era `llama-server/local`; MCP estaba desactivado.

```text
Abre solo README.md con una herramienta de lectura y resume su contenido en una
frase. No edites nada ni ejecutes comandos.

• working  1s · esc to interrupt
Vale, te leo README.md sin tocar nada más.
▐view▌README.md▌
│ Fixture local de comparación: esta app conserva el prompt Enter y muestra…
╰──
• working 55s · esc to interrupt

Resumen: Es un fixture local simple que conserva el prompt "Enter" y muestra
las herramientas en el transcript, con la instrucción explícita de no editar
el archivo.

build · llama-server/local · 0.1/70k 0%
```

Observado: el estado de ejecución muestra elapsed time y `esc to interrupt`; el
tool call aparece como chip con nombre/ruta, el resultado como bloque debajo, y
la respuesta final permanece en el transcript. La operación fue solo lectura.

## Crush v0.98.2 — composer vacío

- Binario instalado `crush.exe`; CWD y `--data-dir` bajo
  `%TEMP%\loom-crush-visual-capture`.
- `CRUSH_DISABLE_METRICS=1` y `CRUSH_DISABLE_PROVIDER_AUTO_UPDATE=1`.
- No se envió ningún prompt. La configuración visible eligió
  `Thinking Machines: Inkling (free) via OpenRouter`; por eso no se hizo una
  llamada al modelo.

```text
Charm™ v0.98.2-0.20261017181040-85…
[banner Charm con glifos diagonales]

~\AppData\Local\Temp\loom-crush-visual-capture

◇ Thinking Machines: Inkling (free) via OpenRouter
  Reasoning Medium

LSPs                     MCPs                     Skills
None                     None                     None

> Ready...
:::

tab change focus • / or ctrl+p commands • shift+tab mode • ctrl+l models …
```

Al primer `Ctrl+C`, Crush mostró un diálogo centrado con “Are you sure you want
to quit?”, opciones “Yep!”/“Nope” y la instrucción de usar doble `Ctrl+C` para
salir sin confirmación. El segundo `Ctrl+C` cerró el proceso con código 0.

## Límites pendientes

## Segunda pasada en PTY aislado — 2026-10-18

Se repitió el arranque de ambas aplicaciones en CWD temporales independientes.
No se introdujo ningún prompt. La base de datos de MastraCode también se aisló;
MCP quedó desactivado y se forzó el tema dark. El runner no expone de forma
fiable las dimensiones del terminal, así que estos frames sirven para estudiar
contenido y jerarquía, no para medir geometría exacta.

### MastraCode 0.47.1

Proyecto temporal `mastra-cwd`; se abrió `/help` y luego `/theme`. El picker
mostró 13 opciones, incluida la opción `dark` activa y temas integrados como
Catppuccin, Dracula, Gruvbox, Nord y Solarized. Se cerró el picker sin cambiar
la selección. El pie mostró `build · llama-server/local · 0/70k 0%`.

El help agrupa comandos por función y presenta cada comando con una descripción
alineada en columnas. El selector de temas es una lista navegable con breve
descripción/origen del tema y contador de página. La interacción de ambos
overlays se completó sin llamar al proveedor.

### Crush v0.98.2

En la ejecución previa con CWD y `--data-dir` temporales se abrió `/` y se
recorrió el menú de temas. El listado incluía `charmtone-panther` y
`gruvbox-dark` (marcado como actual). Se salió sin seleccionar otro tema. Se
mantuvieron `CRUSH_DISABLE_METRICS=1` y
`CRUSH_DISABLE_PROVIDER_AUTO_UPDATE=1`; no se envió un prompt.

### Qué falta para comparar fielmente

- Estas son observaciones de salida VT, no imágenes. No permiten comparar
  tipografía, color real, antialiasing ni proporciones de paneles como píxeles.
- La superficie CUA disponible no expone ventanas ni apps para captura nativa;
  REA tampoco puede capturar este proceso TUI en Windows. Por eso no hay PNG.
- Falta capturar ambos programas con el mismo tamaño de terminal y el endpoint
  local configurado para observar un ciclo de herramienta sin usar un proveedor
  externo. Después se podrá contrastar composición, densidad y estados de tool.

- El terminal de Crush tenía seleccionado OpenRouter; no se envió una petición
  externa. Falta configurar una sesión temporal con el endpoint local para
  capturar una llamada de herramienta de Crush.
- La captura de Crush demuestra composer vacío, footer y salida; falta probar
  crecimiento multilínea, navegación, permisos, estados/resultados de tool,
  temas y terminal estrecho.
- REA está instalado, pero su `capture-process` no puede ejecutar TUI Windows.
  Las observaciones anteriores proceden del stream VT de las sesiones PTY
  abiertas por el runner, no de una captura REA ni de screenshot físico.
