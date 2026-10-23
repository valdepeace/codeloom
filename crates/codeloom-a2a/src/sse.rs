//! Decodificador de Server-Sent Events acotado (A8c).
//!
//! Cabe señalar que el flujo viene de un tercero: es dato, nunca instrucción, y no se fía ni de su
//! tamaño ni de que respete el formato. Se decodifica por bytes con dos cotas duras (una línea y un
//! evento completo) y se ignora todo campo que no sea `data`. Solo se entrega el texto de cada
//! evento; interpretarlo es cosa del llamante.
//!
//! Reglas del formato (WHATWG, que es lo que la especificación A2A llama SSE): las líneas terminan en
//! `\n`, `\r\n` o `\r`; una línea que empieza por `:` es un comentario (el latido); `data:` añade una
//! línea de datos (un espacio inicial se quita) y una línea en blanco cierra el evento.

/// Longitud máxima de una línea, en bytes.
pub const MAX_LINEA: usize = 64 * 1024;
/// Tamaño máximo de los datos de un evento, en bytes.
pub const MAX_EVENTO: usize = 256 * 1024;

/// Por qué se corta un flujo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SseError {
    #[error("una línea del flujo supera los {MAX_LINEA} bytes")]
    LineaDemasiadoLarga,
    #[error("un evento del flujo supera los {MAX_EVENTO} bytes")]
    EventoDemasiadoLargo,
    #[error("el flujo no es UTF-8")]
    NoUtf8,
}

/// Acumula bytes y entrega los eventos completos.
#[derive(Debug, Default)]
pub struct SseDecoder {
    /// Bytes de la línea en curso, aún sin terminador.
    linea: Vec<u8>,
    /// Datos del evento en curso (líneas `data:` unidas con `\n`).
    datos: String,
    hay_datos: bool,
    /// El último byte fue `\r`: un `\n` inmediato es parte del mismo terminador.
    tras_cr: bool,
}

impl SseDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Procesa un trozo y devuelve los eventos que se han completado con él.
    pub fn push(&mut self, trozo: &[u8]) -> Result<Vec<String>, SseError> {
        let mut eventos = Vec::new();
        for &byte in trozo {
            if self.tras_cr {
                self.tras_cr = false;
                if byte == b'\n' {
                    continue;
                }
            }
            match byte {
                b'\n' => self.terminar_linea(&mut eventos)?,
                b'\r' => {
                    self.tras_cr = true;
                    self.terminar_linea(&mut eventos)?;
                }
                otro => {
                    if self.linea.len() >= MAX_LINEA {
                        return Err(SseError::LineaDemasiadoLarga);
                    }
                    self.linea.push(otro);
                }
            }
        }
        Ok(eventos)
    }

    fn terminar_linea(&mut self, eventos: &mut Vec<String>) -> Result<(), SseError> {
        let linea = std::mem::take(&mut self.linea);
        if linea.is_empty() {
            // Línea en blanco: cierra el evento, si lo hay.
            if self.hay_datos {
                eventos.push(std::mem::take(&mut self.datos));
                self.hay_datos = false;
            }
            return Ok(());
        }
        let linea = std::str::from_utf8(&linea).map_err(|_| SseError::NoUtf8)?;
        if linea.starts_with(':') {
            return Ok(());
        }
        let (campo, valor) = match linea.split_once(':') {
            Some((campo, valor)) => (campo, valor.strip_prefix(' ').unwrap_or(valor)),
            None => (linea, ""),
        };
        if campo != "data" {
            // `event`, `id`, `retry` y cualquier otro campo: no se usan.
            return Ok(());
        }
        let extra = valor.len() + usize::from(self.hay_datos);
        if self.datos.len() + extra > MAX_EVENTO {
            return Err(SseError::EventoDemasiadoLargo);
        }
        if self.hay_datos {
            self.datos.push('\n');
        }
        self.datos.push_str(valor);
        self.hay_datos = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn todo(entrada: &[u8]) -> Result<Vec<String>, SseError> {
        SseDecoder::new().push(entrada)
    }

    #[test]
    fn a_simple_event_is_delivered_when_its_blank_line_arrives() {
        let mut d = SseDecoder::new();

        assert!(d.push(b"data: {\"a\":1}\n").unwrap().is_empty());
        assert_eq!(d.push(b"\n").unwrap(), ["{\"a\":1}"]);
    }

    #[test]
    fn an_event_split_in_arbitrary_chunks_is_the_same_event() {
        let entrada = b"data: uno\n\ndata: dos\n\n";
        let mut d = SseDecoder::new();
        let mut eventos = Vec::new();
        for trozo in entrada.chunks(3) {
            eventos.extend(d.push(trozo).unwrap());
        }

        assert_eq!(eventos, ["uno", "dos"]);
    }

    #[test]
    fn crlf_cr_and_lf_terminators_all_work_even_when_split() {
        assert_eq!(todo(b"data: a\r\n\r\n").unwrap(), ["a"]);
        assert_eq!(todo(b"data: a\r\r").unwrap(), ["a"]);
        let mut d = SseDecoder::new();
        assert!(d.push(b"data: a\r").unwrap().is_empty());
        // El `\n` que sigue a un `\r` es parte del mismo terminador, no una línea en blanco.
        assert!(d.push(b"\ndata: b\r\n").unwrap().is_empty());
        assert_eq!(d.push(b"\r\n").unwrap(), ["a\nb"]);
    }

    #[test]
    fn heartbeat_comments_and_unknown_fields_are_ignored() {
        let entrada = b": latido\n\nevent: x\nid: 7\nretry: 100\ndata: util\n\n";

        assert_eq!(todo(entrada).unwrap(), ["util"]);
    }

    #[test]
    fn multiline_data_is_joined_with_newlines_and_one_leading_space_is_removed() {
        assert_eq!(todo(b"data: a\ndata:  b\ndata\n\n").unwrap(), ["a\n b\n"]);
    }

    #[test]
    fn a_blank_line_without_data_delivers_nothing_and_an_unfinished_event_is_not_delivered() {
        assert!(todo(b"\n\n\n").unwrap().is_empty());
        assert!(todo(b"data: sin cerrar\n").unwrap().is_empty());
    }

    #[test]
    fn a_line_over_the_cap_is_an_error_even_without_a_terminator() {
        let mut entrada = b"data: ".to_vec();
        entrada.extend(vec![b'x'; MAX_LINEA]);

        assert_eq!(todo(&entrada), Err(SseError::LineaDemasiadoLarga));
    }

    #[test]
    fn an_event_over_the_cap_is_an_error_though_each_line_is_small() {
        let mut d = SseDecoder::new();
        let linea = format!("data: {}\n", "x".repeat(40_000));
        let mut resultado = Ok(Vec::new());
        for _ in 0..8 {
            resultado = d.push(linea.as_bytes());
            if resultado.is_err() {
                break;
            }
        }

        assert_eq!(resultado, Err(SseError::EventoDemasiadoLargo));
    }

    #[test]
    fn invalid_utf8_is_an_error_not_a_panic() {
        assert_eq!(todo(b"data: \xff\xfe\n\n"), Err(SseError::NoUtf8));
    }

    #[test]
    fn many_small_events_do_not_accumulate_state() {
        let mut d = SseDecoder::new();
        for _ in 0..10_000 {
            assert_eq!(d.push(b"data: x\n\n").unwrap(), ["x"]);
        }
    }
}
