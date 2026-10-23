# Matriz de componentes por tema en el showcase de Batten

Estado: diseño, 2026-10-23. Idea de la persona usuaria; sin implementar.

## Propósito

Saber de un vistazo **qué componente existe en qué tema**, sin recorrer los demos uno a uno. Hoy el
showcase es una lista de 130 demos (61 componentes) y la única forma de saber si «Composer» existe
en Pawan es buscarlo a mano.

## Decisiones (de la persona usuaria, 2026-10-23)

1. **Filas** = componentes, a la izquierda. **Columnas** = temas, en una cabecera para
   seleccionarlos.
2. **Tres estados por celda**, tal cual:
   - `●` hay reproducción propia de ese tema.
   - `◐` solo existe el widget genérico, pintado con la paleta del tema.
   - `·` no existe.
3. Columnas: los temas de `ThemePreset`: Batten (el aspecto *classic*), Martty, Mindfork, Pawan,
   Crush y MastraCode. *Interpretación propia:* «classic» es Batten, porque ese nombre solo
   existe como `Look::classic()`; y Mindfork se incluye aunque no se nombró.

## Qué hay hoy (medido el 2026-10-23 sobre `demos/mod.rs`)

El catálogo ya declara `component`, `variant` y `provider` por demo, así que la matriz se deriva
de datos que existen; no hace falta adivinar por nombre de fichero. Cifras: 130 demos, 61
componentes.

- **MastraCode no tiene ningún demo propio**: es solo una paleta. Su columna saldría sin un solo
  `●`, y eso es información, no un fallo.
- **Crush** es la columna con más reproducciones (decenas de componentes); Martty, Mindfork y
  Pawan tienen pocas.
- Hay proveedores que **no son temas**: CodeLoom (12 demos de herramientas) y dotmax (3).

## Diseño

### Origen de los datos

Una función pura sobre `catalog()` que devuelve, por componente y tema, un `Cell`:
`Own(n)` (n demos del proveedor de ese tema), `Generic` o `Missing`.

- `Own(n)` si hay al menos un demo con ese `component` y el proveedor del tema.
- `Generic` si no hay propio **pero existe un demo del proveedor Batten** para ese componente: se
  asume que ese widget se pinta con la paleta activa porque `Demo::render` recibe el `Theme`.
- `Missing` en el resto. **No se promociona** un demo de otro proveedor a `Generic`: no consta que
  respete el tema activo (p. ej. un demo de Crush puede llevar sus colores fijos).

### Estricta con el proveedor (hallazgo)

`provider_for` devuelve «Batten» **por defecto** cuando el nombre de la variante no empieza por un
proveedor conocido. Hoy eso afecta a 6 entradas: cinco con componente «Batten — primitivas
exclusivas» y variantes Button, File changes, Toast, Effects y Themes, y una, «Tool activity»,
con la variante «Cross-reference comparison». La matriz **no usa ese valor por defecto**: usa una
variante de `provider_for` que devuelve `Option` y deja esas entradas fuera de las columnas (en una
nota al pie), de modo que un demo mal nombrado no infle la columna de Batten en silencio.

### Filas sin columna

Los componentes cuyos únicos demos son de CodeLoom o dotmax (tools de CodeLoom, Braille,
Progress catalog) no tienen celda en ningún tema. Van a una nota al pie, no a la matriz, para no
llenarla de `·` que no significan «falta» sino «no es un tema».

### Interfaz

La matriz es un **modo del showcase**, no un `Demo`: necesita ver el catálogo entero y cambiar el
tema y la selección, y el trait `Demo` no ve nada de eso. Se abre desde el índice.

- Flechas: mover la celda. Cabecera con los seis temas.
- `Enter` sobre una celda: abre el demo de ese componente **con ese tema aplicado**. Si la celda
  es `Own(n)` con `n > 1`, abre el primero; si es `Generic`, abre el demo de Batten con el tema de
  la columna; si es `Missing`, no hace nada y lo dice en la línea de ayuda.
- Esc: vuelve a la matriz desde el demo.
- La tecla de ciclar tema que ya existe no cambia.
- Pie con la leyenda `● propio  ◐ genérico  · no existe` y el recuento por columna.

### Pruebas

- La derivación es pura y se prueba sin terminal: `crush_composer_is_own`,
  `pawan_composer_is_generic`, `mastracode_has_no_own_cells`,
  `an_unknown_provider_is_left_out_not_counted_as_batten`,
  `generic_needs_a_batten_demo` (un componente solo de Crush no es `Generic` en Pawan).
- Que todo `Own` y `Generic` apunta a un demo que existe en el catálogo.
- Un `TestBackend` de 100×40 que pinta la matriz y contiene los tres símbolos y los seis temas.

## Fuera de alcance, declarado

- **Comprobar que un demo ajeno respeta el tema activo.** No se verifica; por eso no cuenta como
  `Generic`.
- **Ocultar o reordenar filas** por filtro; la primera versión las lista todas en el orden del
  catálogo.
- **Corregir las 6 entradas mal nombradas**: la matriz las deja fuera con un aviso. Renombrarlas
  es una tarea aparte.

## Preguntas abiertas

- ¿Una celda `Own(n)` debe mostrar el número `n` o basta con `●`? Propuesta: solo `●`, y el
  número en el pie al situarse encima.
- ¿Los proveedores CodeLoom y dotmax merecen una columna aparte («sin tema») en vez de la nota al
  pie?
