# Saneado de la salida ANSI — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Que la salida de herramientas (bash, cargo…) se vea limpia: solo texto y colores. Fuera secuencias de cursor, borrado y OSC (títulos, enlaces), caracteres de control y sobrescrituras con `\r` (barras de progreso).

**Architecture:** `widgets::ansi::sanitize(bytes, keep_sgr)` filtra los bytes antes de `ansi-to-tui`; `ansi_lines` siempre parsea la versión saneada y, si el parser falla, cae a texto plano sin ninguna secuencia.

**Tech Stack:** Rust, `ansi-to-tui` 8 (feature `ansi` de batten).

**Spec:** entrada 5 de `docs/specs/2026-10-16-deuda-tecnica.md`.

## Global Constraints

- Comentarios en español. Mensaje de commit: el indicado, tal cual.
- El commit pasa: `cargo test -p batten`, `cargo test -p batten --examples`, `cargo build -p batten --no-default-features`, `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`.
- **Entorno (Git Bash en Windows):** antes de cualquier `cargo`, en el mismo comando:
  `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe'`.
  No ejecutar `cargo build`/`cargo test` sin `-p`. No matar procesos. No crear directorios de target temporales.
- No hacer `git push`. No tocar ficheros fuera de los listados. No editar Rust con scripts.

## Review Focus

- Secuencias sin terminar al final del buffer (streaming) no se cuelan ni hacen pánico. Test en Task 1.
- `\r\n` (Windows) es un salto de línea, no una sobrescritura. Test en Task 1.

---

### Task 1: `sanitize` y `ansi_lines`

**Files:**
- Modify: `crates/batten/src/widgets/ansi.rs`, `docs/specs/2026-10-16-deuda-tecnica.md`

**Interfaces:**
- Produces: `batten::widgets::ansi::sanitize(bytes: &[u8], keep_sgr: bool) -> Vec<u8>`.

- [ ] **Step 1: Sustituir `ansi_lines` por**

```rust
/// Convierte bytes de un terminal en líneas con estilo: se sanean primero
/// (ver `sanitize`) y se interpretan los colores SGR. Si el parser falla,
/// devuelve el texto sin ninguna secuencia.
pub fn ansi_lines(bytes: &[u8]) -> Vec<Line<'static>> {
    let clean = sanitize(bytes, true);
    match clean.into_text() {
        Ok(text) => text.lines,
        Err(_) => String::from_utf8_lossy(&sanitize(bytes, false))
            .lines()
            .map(|l| Line::from(l.to_string()))
            .collect(),
    }
}

/// Deja solo texto, `\n`, `\t` y (si `keep_sgr`) los colores SGR
/// (`ESC [ … m`). Descarta el resto de secuencias CSI (cursor, borrado), las
/// OSC (títulos, enlaces), los escapes de dos bytes y los caracteres de
/// control. Un `\r` suelto sobrescribe la línea en curso (barras de
/// progreso); `\r\n` es un salto de línea. Las secuencias sin terminar al
/// final se descartan.
pub fn sanitize(bytes: &[u8], keep_sgr: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == 0x1b {
            match bytes.get(i + 1) {
                Some(b'[') => {
                    let mut j = i + 2;
                    while j < bytes.len() && !(0x40..=0x7e).contains(&bytes[j]) {
                        j += 1;
                    }
                    if keep_sgr && j < bytes.len() && bytes[j] == b'm' {
                        out.extend_from_slice(&bytes[i..=j]);
                    }
                    i = j + 1;
                }
                Some(b']') => {
                    let mut j = i + 2;
                    while j < bytes.len()
                        && bytes[j] != 0x07
                        && !(bytes[j] == 0x1b && bytes.get(j + 1) == Some(&b'\\'))
                    {
                        j += 1;
                    }
                    i = if j < bytes.len() && bytes[j] == 0x1b {
                        j + 2
                    } else {
                        j + 1
                    };
                }
                Some(_) => i += 2,
                None => i += 1,
            }
            continue;
        }
        match b {
            b'\r' if bytes.get(i + 1) == Some(&b'\n') => {}
            b'\r' => {
                let start = out.iter().rposition(|&c| c == b'\n').map_or(0, |p| p + 1);
                out.truncate(start);
            }
            b'\n' | b'\t' => out.push(b),
            0x00..=0x1f | 0x7f => {}
            _ => out.push(b),
        }
        i += 1;
    }
    out
}
```

(Se borra el comentario `// limitación: fallback con pérdida; ...`.)

- [ ] **Step 2: Tests**

En `mod tests`, añadir:

```rust
    fn clean(b: &[u8]) -> String {
        String::from_utf8(sanitize(b, true)).unwrap()
    }

    #[test]
    fn drops_cursor_and_erase_sequences() {
        assert_eq!(clean(b"a\x1b[2Kb\x1b[1Ac"), "abc");
    }

    #[test]
    fn drops_osc_with_bel_or_st() {
        assert_eq!(clean(b"\x1b]0;titulo\x07hola"), "hola");
        assert_eq!(clean(b"\x1b]8;;http://x\x1b\\link\x1b]8;;\x1b\\"), "link");
    }

    #[test]
    fn carriage_return_overwrites_but_crlf_is_a_newline() {
        let l = ansi_lines(b"50%\r100%\nok");
        assert_eq!(l.iter().map(text).collect::<Vec<_>>(), vec!["100%", "ok"]);
        let l = ansi_lines(b"a\r\nb");
        assert_eq!(l.iter().map(text).collect::<Vec<_>>(), vec!["a", "b"]);
    }

    #[test]
    fn drops_control_chars_and_keeps_tabs() {
        assert_eq!(clean(b"a\x08\x00b\tc"), "ab\tc");
    }

    #[test]
    fn unterminated_sequences_at_the_end_are_dropped() {
        assert_eq!(clean(b"ok\x1b[31"), "ok");
        assert_eq!(clean(b"ok\x1b]0;tit"), "ok");
        assert_eq!(clean(b"ok\x1b"), "ok");
    }

    #[test]
    fn sgr_is_kept_or_stripped_on_demand() {
        assert_eq!(clean(b"\x1b[31mred\x1b[0m"), "\x1b[31mred\x1b[0m");
        assert_eq!(
            String::from_utf8(sanitize(b"\x1b[31mred\x1b[0m", false)).unwrap(),
            "red"
        );
    }
```

(Si el helper `text` del módulo recibe `&Line`, `l.iter().map(text)` funciona tal cual; si no compila por la firma, usar `l.iter().map(|x| text(x))`.)

- [ ] **Step 3: Deuda técnica**

En `docs/specs/2026-10-16-deuda-tecnica.md`: borrar la fila que empieza por `| 5 |` y cambiar en la línea de resueltas `y 4 (ver docs/plans/2026-10-16-color-depth.md).` por `, 4 (ver docs/plans/2026-10-16-color-depth.md) y 5 (ver docs/plans/2026-10-16-ansi-sanitize.md).`

- [ ] **Step 4: Checks**

Run: `cargo test -p batten && cargo test -p batten --examples && cargo build -p batten --no-default-features && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS. El test existente `plain_and_broken_sequences_keep_text` (`\x1b[999zok` → contiene `ok`) debe seguir pasando.

- [ ] **Step 5: Commit**

```bash
git add crates/batten/src/widgets/ansi.rs docs/specs/2026-10-16-deuda-tecnica.md
git commit -m "feat(batten): sanear la salida de terminal antes de interpretar colores"
```
