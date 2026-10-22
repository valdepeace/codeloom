# Compactación ligera y topes de contexto

Estado: propuesta del 2026-10-22, para implementar de inmediato. Desarrolla las reglas 3, 5 y 8 de
`2026-10-22-codeloom-decalogo-de-contexto-y-coste.md`; fuentes en
`docs/references/contexto-y-coste/fuentes.md`.

## Intención

Cada paso del agente reenvía **todo** el historial. Tras el incidente del `glob` de seis megabytes
se acotó la salida de cada herramienta, pero quedan tres vías por las que el contexto crece sin
control: el historial acumulado, los avisos de subagentes y jobs (entran como mensajes de usuario,
no como resultado de herramienta, y por eso el tope no los alcanza) y las coincidencias de `grep`.

## Qué dice la referencia y qué es propio

Anthropic recomienda compactar el historial y señala como forma «más segura y ligera» **limpiar los
resultados de herramientas antiguos**; y que un subagente devuelva un resumen de unos 1 000-2 000
tokens. No fija umbrales ni cifras de compactación. **Todas las cifras de este documento son
decisiones propias.**

## Decisiones (propias)

1. **Limpieza de resultados antiguos, solo en la petición.** Al construir cada petición al
   proveedor, los resultados de herramienta anteriores a los `KEEP_TOOL_RESULTS = 6` más recientes
   cuyo contenido supere 256 bytes se sustituyen por
   `[tool result cleared: {n} bytes; run the tool again if you need it]`. **El historial guardado
   no se toca**: la persona usuaria sigue viendo todo y la sesión no pierde nada. Se conserva cada
   `call_id` y el orden, para que el emparejamiento llamada-resultado siga siendo válido.
2. **Tope de los avisos.** El texto que un subagente o un job devuelve al padre dentro de un aviso
   se recorta a `MAX_NOTICE_BYTES = 8 000` (unos 2 000 tokens) con el mismo aviso de recorte que ya
   usa `limit_output`.
3. **`grep` con máximo propio:** 200 coincidencias, cada línea recortada a 300 caracteres, con
   aviso de cuántas había.

## Fuera de alcance, y por qué

- **Resumen del historial por el modelo** (compactación completa): requiere una llamada extra al
  proveedor y diseño propio; se aborda si la limpieza no basta.
- **Presupuesto de tokens por turno o sesión:** requiere decidir qué ocurre al alcanzarlo (parar,
  preguntar a la persona usuaria, avisar). Es decisión de producto: queda como pregunta abierta.
- Configuración por la persona usuaria de las cifras: constantes por ahora.

## Anti-objetivos

- Que la limpieza altere lo persistido o lo que se muestra en la interfaz.
- Dejar un `tool_call` sin su resultado, o al revés (los proveedores lo rechazan).
- Recortar en mitad de un carácter UTF-8.

## Verificación

- Un historial con 10 resultados grandes produce una petición con los 4 más antiguos sustituidos y
  los 6 recientes intactos; el historial original queda igual; los `call_id` se conservan.
- Un resultado pequeño (≤ 256 bytes) antiguo no se sustituye.
- Un aviso de subagente con 50 000 bytes llega al historial recortado a 8 000 más el aviso.
- `grep` sobre un directorio con miles de coincidencias devuelve 200 y el aviso.

## Preguntas abiertas

- Al alcanzar un presupuesto de tokens: ¿parar el turno, preguntar, o solo avisar?
- ¿Basta la limpieza de resultados o hará falta resumir con el modelo? Se mide con sesiones reales.
