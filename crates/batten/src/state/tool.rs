use super::tool_atom::ToolFacts;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
    Pending,
    Running,
    Success,
    Error,
    Canceled,
    AwaitingPermission,
}

#[derive(Debug, Clone)]
pub struct ToolItem {
    pub call_id: String,
    pub name: String,
    pub expanded: bool,
    pub summary: Option<String>,
    /// Hechos extraídos de la entrada de la llamada, calculados una sola vez al
    /// fijarla (ver `codeloom-tui`). Los usa la pieza de render de la herramienta.
    pub facts: ToolFacts,
    pub output: Option<String>,
    started: bool,
    result: Option<bool>,
    canceled: bool,
    awaiting_permission: bool,
}

impl ToolItem {
    pub fn new(call_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            call_id: call_id.into(),
            name: name.into(),
            expanded: false,
            summary: None,
            facts: ToolFacts::default(),
            output: None,
            started: false,
            result: None,
            canceled: false,
            awaiting_permission: false,
        }
    }

    pub fn start(&mut self) {
        self.started = true;
    }
    pub fn await_permission(&mut self) {
        self.awaiting_permission = true;
    }
    pub fn resolve_awaiting(&mut self) {
        self.awaiting_permission = false;
    }
    pub fn set_result(&mut self, is_error: bool) {
        self.result = Some(is_error);
        // Un resultado implica que la tool termino: ya no espera permiso.
        self.awaiting_permission = false;
        self.started = true;
    }
    pub fn set_summary(&mut self, summary: impl Into<String>) {
        self.summary = Some(summary.into());
    }
    pub fn set_output(&mut self, output: impl Into<String>) {
        self.output = Some(output.into());
    }
    pub fn cancel(&mut self) {
        self.canceled = true;
    }
    pub fn toggle_expanded(&mut self) {
        self.expanded = !self.expanded;
    }

    /// Estado derivado: el resultado y la cancelacion mandan sobre lo almacenado
    /// para que transiciones fuera de orden o duplicadas queden consistentes.
    pub fn status(&self) -> ToolStatus {
        if self.canceled {
            return ToolStatus::Canceled;
        }
        if let Some(is_error) = self.result {
            return if is_error {
                ToolStatus::Error
            } else {
                ToolStatus::Success
            };
        }
        if self.awaiting_permission {
            return ToolStatus::AwaitingPermission;
        }
        if self.started {
            ToolStatus::Running
        } else {
            ToolStatus::Pending
        }
    }

    pub fn is_spinning(&self) -> bool {
        self.result.is_none() && !self.canceled && !self.awaiting_permission
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_pending_running_success() {
        let mut t = ToolItem::new("c1", "view");
        assert_eq!(t.status(), ToolStatus::Pending);
        assert!(t.is_spinning());
        t.start();
        assert_eq!(t.status(), ToolStatus::Running);
        assert!(t.is_spinning());
        t.set_result(false);
        assert_eq!(t.status(), ToolStatus::Success);
        assert!(!t.is_spinning());
    }

    #[test]
    fn result_is_derived_even_out_of_order() {
        let mut t = ToolItem::new("c1", "bash");
        t.set_result(true);
        assert_eq!(t.status(), ToolStatus::Error);
        assert!(!t.is_spinning());
        t.set_result(false);
        assert_eq!(t.status(), ToolStatus::Success);
    }

    #[test]
    fn canceled_wins_over_everything_and_never_spins() {
        let mut t = ToolItem::new("c1", "edit");
        t.start();
        t.set_result(false);
        t.cancel();
        assert_eq!(t.status(), ToolStatus::Canceled);
        assert!(!t.is_spinning());
    }

    #[test]
    fn awaiting_permission_is_not_spinning() {
        let mut t = ToolItem::new("c1", "bash");
        t.start();
        t.await_permission();
        assert_eq!(t.status(), ToolStatus::AwaitingPermission);
        assert!(!t.is_spinning());
        t.resolve_awaiting();
        assert_eq!(t.status(), ToolStatus::Running);
    }

    #[test]
    fn toggle_expanded() {
        let mut t = ToolItem::new("c1", "view");
        assert!(!t.expanded);
        t.toggle_expanded();
        assert!(t.expanded);
    }
}
