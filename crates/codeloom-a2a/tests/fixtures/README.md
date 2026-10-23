# Fixtures de A2A

Procedencia de cada fichero, para no confundir lo literal con lo compuesto.

- `a2a.proto`: descargado el 2026-10-23 de `https://a2a-protocol.org/latest/spec/a2a.proto`
  (paquete `lf.a2a.v1`). Literal.
- `agent-card.sample.json`: la tarjeta de ejemplo de la sección 8.5 de la especificación, copiada
  tal cual (`https://raw.githubusercontent.com/a2aproject/A2A/main/docs/specification.md`, rama
  `main`, que se anuncia como «Latest Released Version 1.0.0»). Literal.
- `jsonrpc-get-task.request.json`, `jsonrpc-cancel-task.request.json`: peticiones de las secciones
  9.4.3 y 9.4.5. Literales.
- `jsonrpc-error-task-not-found.json`, `jsonrpc-error-invalid-params.json`: respuestas de error de
  la sección 9.5. Literales.
- `jsonrpc-send-message.request.json`, `jsonrpc-send-message.response.json`: **compuestos**. La
  especificación solo da el sobre JSON-RPC (sección 9.4.1) con el cuerpo como comentario; el cuerpo
  es el de la sección 6.1 (ejemplo de REST). Son verosímiles, no un literal del binding JSON-RPC.
