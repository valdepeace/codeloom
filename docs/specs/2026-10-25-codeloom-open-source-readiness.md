# Preparación de CodeLoom para abrir el repositorio

Fecha: 2026-10-25. Estado: en preparación; el remoto continúa privado.

## Objetivo

Dejar el repositorio entendible, contribuible y comprobable por terceros, conservando los 421
commits y sin publicar una release ni paquetes en crates.io todavía.

## Decisiones

- **D1. Se conserva el historial.** No se reescriben commits ni se cambian sus SHA.
- **D2. El remoto sigue privado hasta revisar la exposición del correo antiguo de autor.** Cambiar el
  correo configurado ahora solo afecta a commits futuros.
- **D3. Licencia dual.** Se conserva `MIT OR Apache-2.0`; el aviso de copyright MIT identifica a
  `Andres Carmona` y el año 2026. El texto estándar Apache-2.0 se mantiene sin modificar.
- **D4. Idioma.** README y documentos de contribución se mantienen en español, como el resto del repo.
- **D5. Checks públicos.** CI ejecuta los cuatro comandos exigidos (`cargo build`, `cargo test`,
  `cargo fmt --check`, `cargo clippy -- -D warnings`) en Ubuntu y Windows con Rust estable.
- **D6. Reportes.** El código de conducta dirige al perfil de GitHub de `valdepeace`; `SECURITY.md`
  pide divulgación privada y enlaza al reporte privado de GitHub cuando la función esté habilitada.
  Ninguno invita a publicar detalles sensibles en issues.
- **D7. Primera distribución.** La primera release se hará como GitHub Release con tag, notas y
  artefactos solo después de decidir su alcance y confirmar CI. crates.io queda fuera de esta tarea:
  los crates internos dependen entre sí por rutas locales.
- **D8. Fixture mTLS.** Se conserva la identidad PEM de pruebas ya incluida en el historial; está
  explícitamente etiquetada como identidad generada para tests, no como credencial operativa.
- **D9. Correo futuro.** Configurar el correo `noreply` de GitHub solo para este repositorio; no
  modificar el autor de los commits existentes.

## Criterios de aceptación

- README enlaza licencia, contribución, conducta, seguridad y changelog.
- `.gitignore` excluye `.env` y `.env.*`, permitiendo versionar una plantilla `.env.example` vacía.
- CONTRIBUTING explica el flujo mínimo y los cuatro checks del proyecto.
- SECURITY explica cómo reportar problemas sin publicar un exploit.
- CHANGELOG tiene una sección `Unreleased` y una estructura simple por versión.
- El workflow corre los cuatro checks en PRs y pushes a `master`, tanto en Ubuntu como en Windows.
- Los cuatro checks pasan localmente antes del commit.
- Solo se incorporan los archivos de esta preparación; `codeloom.toml` y el test local sin seguimiento
  permanecen fuera.
- El remoto sigue privado; no se crea tag/release ni se publica en crates.io.

## Fuera de alcance

- Reescribir o borrar los 421 commits.
- Hacer público el repo antes de la decisión final sobre el correo histórico.
- Publicar binarios o crates, fijar soporte de plataformas adicionales o automatizar versiones.
- Cambiar código de producto o los cambios locales existentes.
