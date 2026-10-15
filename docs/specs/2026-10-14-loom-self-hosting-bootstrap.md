# CodeLoom — Autoimplementación: ¿hay un mínimo para que se construya a sí mismo?

> Estado: **evaluación**.
>
> Fecha: 2026-10-14
>
> Relacionado: `docs/plans/2026-10-13-loom-mvp.md` (m0+m1),
> `2026-10-14-loom-architecture-overview.md`.

## 0. La pregunta

¿Existe ya un mínimo para que, con un **prompt** y las **herramientas**
necesarias, CodeLoom (o un agente) vaya **implementando las funcionalidades** — es
decir, para que CodeLoom se haga **a sí mismo** (autoimplementación / dogfooding)?

## 1. Estado actual (medido)

- **Solo documentación**: `README.md`, `docs/specs/` (13 specs), `docs/plans/`
  (1 plan: m0+m1).
- **Sin código**: no hay `Cargo.toml`, ni `crates/`, ni `src/`. Nada compila.
- **Sin `AGENTS.md`** (el fichero de contexto que el plan ya asume).

Conclusión: **todavía no hay mínimo**. Ningún agente puede "entrar y construir"
sin, como poco, el plan m0+m1 ejecutado y un contexto que lo guíe.

## 2. El huevo y la gallina (bootstrap)

CodeLoom **no puede construirse desde cero**: para escribir código necesita ser ya un
agente que lee, edita y ejecuta. Hay dos fases:

1. **Bootstrap (fuera de CodeLoom)**: otro agente (Claude Code, Crush, o cualquiera)
   —o hacerlo a mano— ejecuta el **m0+m1** del plan hasta tener CodeLoom ejecutable.
2. **Dogfooding (dentro de CodeLoom)**: una vez en m1, **CodeLoom se implementa a sí
   mismo** el m2 (subagentes/jobs), m3 (herdr) y m4 (MCP/LSP/skills/memoria), con
   un prompt y sus tools.

El mínimo es, literalmente, **llegar al m1 MVP**: no antes se toca a sí mismo.

## 3. El mínimo autosuficiente (criterios)

Para que CodeLoom pueda trabajar en su propio repo debe poder:

- **Leer**: `view`, `glob`, `grep`.
- **Editar**: `edit` (find/replace exacto).
- **Ejecutar**: `bash` con `cargo build`, `cargo test`, `cargo fmt --check`,
  `cargo clippy -- -D warnings` (los comandos del plan).
- **Hablar con un proveedor** (openai_compat o anthropic).
- **Cargar contexto**: leer `AGENTS.md` + los specs/plan.
- **Respetar reglas duras**: `codeloom-core` **no** depende de `ratatui`/`crossterm`;
  errores de librería con `thiserror`; escritura de herramientas `Exclusive`.
- **Permisos a prueba de balas** (gate `tool.risk`): se **auto-modifica**, así que
  el gate y el modo `shadow` son doblemente importantes aquí.

## 4. El prompt y las herramientas

- **El prompt = `AGENTS.md`.** Debe decir: qué es CodeLoom, dónde viven los specs y el
  plan, los comandos de build/test/lint, la nomenclatura (CodeLoom, `codeloom-herdr`), las
  reglas de arquitectura y las trampas conocidas. **Hoy no existe**: es el
  artefacto que falta para el "prompt".
- **Las tools**: las 5 del MVP (view/edit/bash/glob/grep). Idealmente, además, la
  tool `todos` y el gate `tool.risk` de jevalan desde el principio.
- **El trabajo lo guía el plan**: `docs/plans/2026-10-13-loom-mvp.md` ya está
  escrito task-by-task con pasos y verificación (`cargo build`, `cargo test`).

## 5. Plan de bootstrap (pasos concretos)

1. **Escribir `AGENTS.md`** (contexto para cualquier agente). Barato y
   desbloquea todo lo demás.
2. **Ejecutar m0+m1** del plan con **otro** agente (o a mano) hasta que
   `cargo run -p codeloom-cli` funcione y pase `cargo test`.
3. **Dogfooding**: a partir de ahí, usar CodeLoom sobre su propio repo para m2+,
   empezando por tareas pequeñas y con el gate en `shadow`.

## 6. Riesgos

- **Bootstrap**: ¿quién lo hace? Sin un agente externo, no arranca (huevo/gallina).
- **Auto-modificación**: un agente que edita su propio motor puede romperse. Gate
  + `shadow` + tests por commit + git (poder volver atrás).
- **Calidad**: el plan exige `cargo fmt/clippy/test` por commit; sin eso, CodeLoom se
  degrada a sí mismo.
- **Contexto**: sin `AGENTS.md` y specs curados, cada sesión re-descubre el
  proyecto (y quema tokens).

## 7. Recomendación

**Sí hay camino, pero falta el arranque.** Orden: **(1)** crear `AGENTS.md`,
**(2)** ejecutar el m0+m1, **(3)** dogfooding. El primer paso es el más barato y
el que habilita "un prompt y las herramientas": sin él, no hay prompt que valga.

> Nota: este es el mismo `AGENTS.md` que pedía la primera tarea de esta sesión;
> entonces el repo solo tenía docs, así que se dejó pendiente. Ahora se crea con
> lo observado: specs, plan, comandos, reglas y trampas.
