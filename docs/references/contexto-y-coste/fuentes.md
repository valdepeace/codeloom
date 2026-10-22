# Fuentes consultadas: contexto y coste

Consultadas el 2026-10-22 para contrastar `docs/specs/2026-10-22-codeloom-decalogo-de-contexto-y-coste.md`.
Cabe señalar que las citas son casi textuales y breves; lo parafraseado se dice como tal. Las
páginas externas cambian: se anota la versión o la fecha que mostraban.

## Leídas (primarias)

| Fuente | URL | Versión o fecha | Qué respalda |
|---|---|---|---|
| OWASP, LLM10:2025 «Unbounded Consumption» | https://genai.owasp.org/llmrisk/llm102025-unbounded-consumption/ | edición 2025 | Límites de tamaño de entrada, tasa y cuotas, timeouts y *throttling*, acciones en cola. **No** cubre tokens, salida, pasos ni tamaño de resultados de herramientas. |
| MCP, Tools | https://modelcontextprotocol.io/specification/latest/server/tools | 2026-07-28 | `tools/call` sin límite ni truncado de resultado. Servidor: limitar tasa, sanear salidas; cliente: validar resultados, timeouts. |
| MCP, Pagination | https://modelcontextprotocol.io/specification/2026-07-28/server/utilities/pagination.md | 2026-07-28 | Paginación solo en listados (`tools/list`, `resources/list`…); tamaño de página decidido por el servidor. |
| MCP, Security best practices | https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices.md | 2026-07-28 | Nada sobre tamaño de respuestas. |
| MCP, Client best practices (no normativa) | https://modelcontextprotocol.io/docs/2026-07-28/develop/clients/client-best-practices.md | 2026-07-28 | «Validate and truncate sandbox console output before feeding it back to the model». |
| Anthropic, Writing effective tools for AI agents | https://www.anthropic.com/engineering/writing-tools-for-agents | 2025-09-11 | 25 000 tokens por defecto en Claude Code; paginación, rangos, filtros y truncado con valores por defecto sensatos; errores accionables. |
| Anthropic, Effective context engineering for AI agents | https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents | 2025-09-29 | Compactación del historial, limpieza de resultados antiguos, subagentes con resúmenes de 1 000-2 000 tokens. |

## Apuntadas y no leídas

- https://www.anthropic.com/news/context-management (limpieza de resultados de herramientas, enlazada desde la guía de contexto).
- https://www.anthropic.com/engineering/code-execution-with-mcp
- https://www.anthropic.com/engineering/multi-agent-research-system
- https://modelcontextprotocol.io/llms.txt (solo el índice; parte es resumen generado, no se cita).
- Páginas de Resources y de autorización de MCP, y el esquema (`schema.md`): no se abrieron, no se afirma nada sobre ellas.
- OWASP referencia además CWE-400 y OWASP API4:2023 «Unrestricted Resource Consumption»: no se abrieron.

## Dirección que ya no existe

- https://modelcontextprotocol.io/specification/latest/basic/utilities/pagination da 404; la vigente es la de `2026-07-28` de la tabla.

## Secundarias (no se citan como respaldo)

Resúmenes de terceros que dieron la numeración errónea «LLM06:2026» y que no se usan como
fuente: superwise.ai, cycode.com, dev.to/lusivision, aigl.blog, getmaxim.ai, imperva.com.
