//! Copia al portapapeles con OSC 52: funciona por SSH y sin dependencias del
//! sistema; el terminal decide si lo acepta.

use base64::Engine;

/// Secuencia OSC 52 que pide al terminal copiar `text` al portapapeles.
pub fn osc52(text: &str) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
    format!("\x1b]52;c;{b64}\x07")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_utf8_as_base64() {
        assert_eq!(osc52("hi"), "\x1b]52;c;aGk=\x07");
        assert_eq!(osc52("ñ"), "\x1b]52;c;w7E=\x07");
    }
}
