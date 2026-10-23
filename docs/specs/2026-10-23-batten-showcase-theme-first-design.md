# Showcase de Batten: el tema primero

Estado: diseño, 2026-10-23. Corrige el diseño de
`2026-10-23-batten-showcase-theme-matrix-design.md` en lo que la persona usuaria declaró erróneo.

## Propósito y corrección

La persona usuaria lo dijo así: se elige el **tema** (en la cabecera o en el pie) y se ven
**todos los componentes de ese tema**; *no* al revés, un componente con todos los temas dentro.
«Por diseño eso está mal.»

Hoy el índice lateral agrupa por componente y, dentro, lista una variante por proveedor
(Batten, Mindfork, Martty, Crush…): es exactamente el diseño rechazado. La entrada de teclado
`F8` (saltar entre temas dentro de un componente) y la fila de proveedores como filtro son del
mismo diseño.

## Decisiones (de la persona usuaria, 2026-10-23)

1. **Selector de tema en la cabecera** (pestañas), con la opción de verlo también en el pie.
2. **La barra lateral lista componentes**, no variantes por proveedor. Un componente solo tiene
   filas hijas **cuando tiene varios ejemplos** en ese tema.
3. Al cambiar de tema, la barra lateral muestra los componentes **de ese tema**.

## Decisiones propias (corregibles)

- **Qué componentes aparecen en un tema**: los que tienen demo propio (`●`) y los que solo tienen
  el genérico de Batten pintado con esa paleta (`◐`, marcado). Los que no existen en ese tema
  **no aparecen**; el pie cuenta cuántos faltan y remite a la matriz (`m`), que sigue siendo la
  vista de huecos. Es la misma regla de `Own` / `Generic` / `Missing` ya implementada.
- **Pestañas**: los siete temas de `ThemePreset`. dotmax no es un tema y sigue en la nota de la
  matriz.
- **Qué pasa con la selección al cambiar de tema**: si el componente seleccionado existe en el
  nuevo tema, se conserva (con su primer ejemplo); si no, se pasa al primero de la lista.
- **Subfilas**: el texto de un ejemplo es la variante sin el prefijo del proveedor
  (`Crush · pending` → `pending`); si queda vacío, `principal`.
- **Teclas**: `←`/`→` (y `F7` / `Shift+F7`, que ya existen) cambian de tema en el índice y en el
  demo; `↑`/`↓` mueven por componentes y ejemplos. `F8` y la tecla `p` (filtro por proveedor)
  desaparecen. `/` filtra por nombre de componente. `m` abre la matriz como hasta ahora.
- **Pie**: el nombre del tema, el recuento («26 componentes: 9 ● · 17 ◐ · faltan 31, `m` para
  verlos») y, si el seleccionado es genérico, la nota «◐ parcialmente implementado (genérico de
  Batten con la paleta de {tema})».

## Ratón

Hoy el showcase solo atiende el clic izquierdo; ignora la rueda y el arrastre, y con la matriz
abierta un clic en el índice o en el panel la cierra sin sentido. Nuevo comportamiento:

- Clic en una pestaña de tema: cambia de tema.
- Clic en un componente o ejemplo de la barra lateral: lo selecciona y abre el demo.
- Rueda sobre la barra lateral: mueve la selección; sobre la matriz, el cursor.
- Clic en una celda de la matriz: mueve el cursor; doble pulsación no: `Enter` abre.
- Clic en el panel del demo: da el foco al demo, como ahora.

*No se ha podido reproducir en una terminal real* el fallo que la persona usuaria describió como
«no funciona el ratón»; lo que se corrige es lo que el código demuestra mal (rueda, matriz).
Sigue pendiente saber qué acción concreta no respondía.

## Fuera de alcance, declarado

- Reenviar el ratón a los demos (arrastres dentro de un widget).
- Cambiar los demos existentes o el catálogo.
- Comprobar que un demo genérico respeta el tema activo.
