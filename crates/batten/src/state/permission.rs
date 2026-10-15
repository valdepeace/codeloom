//! Estado del diálogo de permisos (sin teclas: la app traduce su teclado).

use super::diff::Diff;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PermissionChoice {
    #[default]
    Allow,
    AllowSession,
    Deny,
}

impl PermissionChoice {
    const ALL: [PermissionChoice; 3] = [
        PermissionChoice::Allow,
        PermissionChoice::AllowSession,
        PermissionChoice::Deny,
    ];

    /// Posición en la fila de botones.
    pub fn index(self) -> usize {
        match self {
            PermissionChoice::Allow => 0,
            PermissionChoice::AllowSession => 1,
            PermissionChoice::Deny => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PermissionBody {
    Text(String),
    Diff(Diff),
}

#[derive(Debug, Clone)]
pub struct PermissionDialog {
    pub tool: String,
    /// Línea secundaria (p. ej. la ruta); vacía si no hay.
    pub subtitle: String,
    pub body: PermissionBody,
    choice: PermissionChoice,
    scroll: usize,
}

impl PermissionDialog {
    pub fn new(tool: impl Into<String>, subtitle: impl Into<String>, body: PermissionBody) -> Self {
        Self {
            tool: tool.into(),
            subtitle: subtitle.into(),
            body,
            choice: PermissionChoice::default(),
            scroll: 0,
        }
    }

    pub fn choice(&self) -> PermissionChoice {
        self.choice
    }

    /// Siguiente opción (cíclica).
    pub fn next(&mut self) {
        self.choice = PermissionChoice::ALL[(self.choice.index() + 1) % 3];
    }

    /// Opción anterior (cíclica).
    pub fn prev(&mut self) {
        self.choice = PermissionChoice::ALL[(self.choice.index() + 2) % 3];
    }

    /// Desplaza el cuerpo; nunca baja de 0.
    pub fn scroll_by(&mut self, delta: isize) {
        self.scroll = self.scroll.saturating_add_signed(delta);
    }

    pub fn scroll(&self) -> usize {
        self.scroll
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dialog() -> PermissionDialog {
        PermissionDialog::new("bash", "", PermissionBody::Text("ls".into()))
    }

    #[test]
    fn choices_cycle_both_ways() {
        let mut d = dialog();
        assert_eq!(d.choice(), PermissionChoice::Allow);
        d.next();
        assert_eq!(d.choice(), PermissionChoice::AllowSession);
        d.next();
        d.next();
        assert_eq!(d.choice(), PermissionChoice::Allow);
        d.prev();
        assert_eq!(d.choice(), PermissionChoice::Deny);
    }

    #[test]
    fn scroll_never_goes_negative() {
        let mut d = dialog();
        d.scroll_by(-3);
        assert_eq!(d.scroll(), 0);
        d.scroll_by(5);
        d.scroll_by(-2);
        assert_eq!(d.scroll(), 3);
    }
}
