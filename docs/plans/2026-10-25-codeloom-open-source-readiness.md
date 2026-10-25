# Plan: preparación Open Source de CodeLoom

Spec: `docs/specs/2026-10-25-codeloom-open-source-readiness.md`.

## Tareas

1. Añadir los textos MIT y Apache-2.0; el aviso MIT identifica a Andres Carmona (2026).
2. Añadir `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md` y `CHANGELOG.md` en español.
3. Enlazar esos documentos desde el README.
4. Ignorar `.env.*` y añadir una plantilla `.env.example` sin secretos.
5. Añadir CI de GitHub Actions para los cuatro checks en Ubuntu y Windows.
6. Configurar el correo GitHub `noreply` solo en `.git/config` de este repo.
7. Ejecutar los cuatro checks, revisar el diff y confirmar que los cambios locales previos no entran.
8. Commitear y subir solo estos archivos al remoto privado.
9. Resolver los diagnósticos de Clippy actual y sincronizar el test de cancelación con la asignación
   del id remoto; repetir CI sin reducir la cobertura.

## Límites

- No tocar el historial anterior ni incluir `codeloom.toml` o `crates/codeloom-tui/tests/`.
- No cambiar la visibilidad, crear releases/tags ni publicar paquetes.
- Antes de abrir el repo, confirmar que el correo histórico puede ser público y habilitar el reporte
  privado de vulnerabilidades de GitHub.
