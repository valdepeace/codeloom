//! Tono semántico compartido por las primitivas (sin color).

/// Intención de un elemento; el widget la resuelve a estilo, icono y etiqueta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tone {
    #[default]
    Neutral,
    Info,
    Success,
    Warning,
    Error,
}

impl Tone {
    /// Los cinco tonos, en orden (para demos y tests).
    pub const ALL: [Tone; 5] = [
        Tone::Neutral,
        Tone::Info,
        Tone::Success,
        Tone::Warning,
        Tone::Error,
    ];
}
