//! Modelo puro del sidebar: logo + secciones. Sin ratatui.

/// Marca del sidebar. Solo el nombre; el dibujo lo pone `widgets::logo`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Logo {
    pub name: String,
}

/// Sección del sidebar: un título y sus líneas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub title: String,
    pub lines: Vec<String>,
}

/// Sidebar genérico: logo opcional + secciones en orden.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Sidebar {
    pub logo: Option<Logo>,
    pub sections: Vec<Section>,
}

impl Sidebar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_logo(name: impl Into<String>) -> Self {
        Self {
            logo: Some(Logo { name: name.into() }),
            sections: Vec::new(),
        }
    }

    pub fn push_section(&mut self, title: impl Into<String>, lines: Vec<String>) {
        self.sections.push(Section {
            title: title.into(),
            lines,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_has_no_logo_and_no_sections() {
        let sb = Sidebar::new();
        assert!(sb.logo.is_none());
        assert!(sb.sections.is_empty());
    }

    #[test]
    fn with_logo_sets_the_name() {
        let sb = Sidebar::with_logo("batten");
        assert_eq!(sb.logo.as_ref().unwrap().name, "batten");
    }

    #[test]
    fn push_section_keeps_order_and_content() {
        let mut sb = Sidebar::new();
        sb.push_section("estado", vec!["working".into()]);
        sb.push_section("modelo", vec!["mock".into(), "x".into()]);
        assert_eq!(sb.sections.len(), 2);
        assert_eq!(sb.sections[0].title, "estado");
        assert_eq!(sb.sections[0].lines, vec!["working".to_string()]);
        assert_eq!(sb.sections[1].title, "modelo");
        assert_eq!(sb.sections[1].lines.len(), 2);
    }
}
