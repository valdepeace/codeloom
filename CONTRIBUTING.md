# Contribuir a CodeLoom

Gracias por considerar una contribución. Para cambios grandes, abre primero un issue y acordemos
el alcance antes de implementar.

## Preparar el entorno

Instala Rust estable (la versión activa se declara en `rust-toolchain.toml`) y clona el repositorio.
La puesta en marcha está en el [README](README.md).

## Antes de enviar un cambio

Ejecuta desde la raíz del workspace los cuatro checks del proyecto:

```bash
cargo build
cargo test
cargo fmt --check
cargo clippy -- -D warnings
```

Las pruebas no deben usar la red real: usa `wiremock` o un `Provider` de prueba. Mantén los límites
de contexto y coste descritos en `docs/specs/2026-10-22-codeloom-decalogo-de-contexto-y-coste.md`.
`codeloom-core` no debe depender de `ratatui` ni `crossterm`; las integraciones van en sus crates.

Para cambios de comportamiento, actualiza la documentación relevante y acompaña el cambio con su
spec y plan en `docs/specs/` y `docs/plans/`, siguiendo los ejemplos existentes.

## Pull requests

- Explica el problema y el resultado esperado.
- Mantén cada cambio enfocado y añade o actualiza pruebas cuando cambie el comportamiento.
- Indica los checks que ejecutaste y cualquier limitación que no hayas podido verificar.
- No incluyas credenciales, `.env`, datos personales ni cambios de configuración local.

Las contribuciones están sujetas al [Código de Conducta](CODE_OF_CONDUCT.md). Para vulnerabilidades,
consulta [SECURITY.md](SECURITY.md) y no abras un issue público con detalles explotables.

Al enviar una contribución para incluirla en CodeLoom, aceptas que se distribuya bajo la misma
licencia dual `MIT OR Apache-2.0`, salvo que acordemos expresamente otra cosa.
