//! Tabla de funcionalidades para el panel de estado de CodeLoom.
//! Muestra el estado actual de implementación de las características del motor y la UI.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

const BG: Color = Color::Rgb(15, 16, 19);
const FG: Color = Color::Rgb(201, 204, 210);
const MUTED: Color = Color::Rgb(133, 139, 146);
const BORDER: Color = Color::Rgb(54, 58, 66);
const GREEN: Color = Color::Rgb(111, 192, 130);
const YELLOW: Color = Color::Rgb(230, 201, 154);
const GRAY: Color = Color::Rgb(88, 94, 105);

/// Estado de una funcionalidad en la tabla.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FuncionalidadEstado {
    /// Implementado completamente (✅)
    Completado,
    /// Parcialmente implementado con limitaciones (🟡)
    Parcial,
    /// Solo presencia visual, sin funcionalidad (🟦)
    Visual,
    /// No implementado (⬜)
    NoImplementado,
}

impl FuncionalidadEstado {
    /// Obtiene el emoji para el estado.
    pub fn emoji(&self) -> &'static str {
        match self {
            Self::Completado => "✅",
            Self::Parcial => "🟡",
            Self::Visual => "🟦",
            Self::NoImplementado => "⬜",
        }
    }

    /// Obtiene el color para el estado.
    pub fn color(&self) -> Color {
        match self {
            Self::Completado => GREEN,
            Self::Parcial => YELLOW,
            Self::Visual => GRAY,
            Self::NoImplementado => Color::White,
        }
    }
}

/// Una entrada en la tabla de funcionalidades.
pub struct FuncionalidadEntrada {
    pub funcionalidad: &'static str,
    pub motor_local: &'static str,
    pub ui: &'static str,
    pub integracion: &'static str,
    pub estado: FuncionalidadEstado,
}

impl FuncionalidadEntrada {
    /// Crea una nueva entrada.
    pub fn new(
        funcionalidad: &'static str,
        motor_local: &'static str,
        ui: &'static str,
        integracion: &'static str,
        estado: FuncionalidadEstado,
    ) -> Self {
        Self {
            funcionalidad,
            motor_local,
            ui,
            integracion,
            estado,
        }
    }
}

/// Widget que muestra la tabla de funcionalidades.
pub fn render_funcionalidad_table(frame: &mut Frame, area: Rect) {
    if area.width < 80 || area.height < 20 {
        return;
    }

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .title(" 📊 Inventario de Funcionalidades ")
        .style(Style::default().bg(BG));

    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    // Encabezados de la tabla
    let header_line = Line::from(vec![
        Span::styled(
            "Funcionalidad",
            Style::default().fg(FG).add_modifier(Modifier::BOLD),
        ),
        Span::raw("     "),
        Span::styled(
            "Motor Local",
            Style::default().fg(FG).add_modifier(Modifier::BOLD),
        ),
        Span::raw("     "),
        Span::styled("UI", Style::default().fg(FG).add_modifier(Modifier::BOLD)),
        Span::raw("     "),
        Span::styled(
            "Integración",
            Style::default().fg(FG).add_modifier(Modifier::BOLD),
        ),
        Span::raw("     "),
        Span::styled(
            "Estado",
            Style::default().fg(FG).add_modifier(Modifier::BOLD),
        ),
    ]);

    let header_para = Paragraph::new(header_line).style(Style::default().bg(BG));
    frame.render_widget(header_para, inner);

    // Línea separadora
    let separator_line = Line::from(vec![Span::styled(
        "─".repeat((inner.width as usize).saturating_sub(3)),
        Style::default().fg(BORDER),
    )]);
    let separator_area = Rect::new(inner.x, inner.y + 1, inner.width, 1);
    frame.render_widget(Paragraph::new(separator_line), separator_area);

    // Entradas de datos
    let funcionalidades = get_funcionalidades();
    let mut current_y = inner.y + 2;
    let line_height: u16 = 2;

    for entrada in funcionalidades {
        let entrada_line = Line::from(vec![
            Span::styled(entrada.funcionalidad, Style::default().fg(FG)),
            Span::raw("     "),
            Span::styled(entrada.motor_local, Style::default().fg(MUTED)),
            Span::raw("     "),
            Span::styled(entrada.ui, Style::default().fg(MUTED)),
            Span::raw("     "),
            Span::styled(entrada.integracion, Style::default().fg(MUTED)),
            Span::raw("     "),
            Span::styled(
                format!("{} {}", entrada.estado.emoji(), entrada.funcionalidad),
                Style::default().fg(entrada.estado.color()),
            ),
        ]);

        let entrada_area = Rect::new(inner.x, current_y, inner.width, line_height);
        frame.render_widget(Paragraph::new(entrada_line), entrada_area);

        current_y += line_height;
    }
}

/// Obtiene las entradas de funcionalidades del inventario.
fn get_funcionalidades() -> Vec<FuncionalidadEntrada> {
    vec![
        FuncionalidadEntrada::new(
            "Envío y respuesta",
            "AgentLoop recibe prompts y ejecuta turnos",
            "Composer multilinea; Enter envía",
            "Canal de prompts hacia el worker y bus de eventos",
            FuncionalidadEstado::Completado,
        ),
        FuncionalidadEntrada::new(
            "Streaming",
            "Adaptadores OpenAI-compatible y Anthropic",
            "Deltas se anexan al transcript",
            "LiveEvent::Delta actualiza la entrada del asistente",
            FuncionalidadEstado::Completado,
        ),
        FuncionalidadEntrada::new(
            "Thinking",
            "Existe ReasoningDelta en el modelo de eventos",
            "Muestra thinking y la última línea recibida",
            "El evento cambia el estado y texto en la app",
            FuncionalidadEstado::Parcial,
        ),
        FuncionalidadEntrada::new(
            "Tools",
            "view, glob, grep, edit, bash; lectura paralela",
            "Lista actividad, estado y resultados; hasta 8 tools",
            "ToolStarted, ToolFinished actualizan Batten",
            FuncionalidadEstado::Completado,
        ),
        FuncionalidadEntrada::new(
            "Permisos",
            "Servicio con permitir, denegar y permitir durante sesión",
            "Diálogo compacto que crece con argumentos/diff",
            "Respuesta del diálogo vuelve al motor antes de ejecutar",
            FuncionalidadEstado::Completado,
        ),
        FuncionalidadEntrada::new(
            "Cancelación",
            "CancellationToken en loop, stream y ejecución de tools",
            "Esc cancela; doble Esc intenta salir",
            "El TUI cancela el token del turno activo",
            FuncionalidadEstado::Parcial,
        ),
        FuncionalidadEntrada::new(
            "Persistencia",
            "SQLite lista sesiones no vacías por proyecto",
            "Selector inicial con preview/cuenta; flechas + Enter",
            "El CLI entrega el historial seleccionado a la app",
            FuncionalidadEstado::Parcial,
        ),
        FuncionalidadEntrada::new(
            "Provider/configuración",
            "codeloom.toml, provider, modelo, URL y API key",
            "Muestra provider/modelo en contexto/sidebar",
            "CLI construye el adaptador y pasa metadatos al TUI",
            FuncionalidadEstado::Parcial,
        ),
        FuncionalidadEntrada::new(
            "Uso de tokens/contexto",
            "Publica uso de tokens",
            "Muestra contadores de entrada/salida",
            "LiveEvent::Usage acumula contadores",
            FuncionalidadEstado::Parcial,
        ),
        FuncionalidadEntrada::new(
            "Archivos cambiados",
            "ChangeTracker y eventos de diff",
            "Panel 'Changed files'",
            "LiveEvent::FileDiff actualiza la lista",
            FuncionalidadEstado::Parcial,
        ),
        FuncionalidadEntrada::new(
            "Estado y errores",
            "Eventos de inicio/fin y error",
            "Estado, animación, thinking y panel de actividad",
            "El bus refleja eventos en el estado visual",
            FuncionalidadEstado::Parcial,
        ),
        FuncionalidadEntrada::new(
            "Cola",
            "El worker consume los prompts recibidos",
            "Cuenta prompts pendientes",
            "El canal comunica prompts y el TUI muestra el contador",
            FuncionalidadEstado::Parcial,
        ),
        FuncionalidadEntrada::new(
            "Flock/subagentes",
            "Estados A2A: el runtime produce 5 de los 8; submitted, auth-required y rejected sin productor",
            "Rail y showcase pintan los 8 estados (glifo, etiqueta y color propios)",
            "No hay eventos/modelos que conecten UI y motor",
            FuncionalidadEstado::Visual,
        ),
        FuncionalidadEntrada::new(
            "Jobs",
            "No hay registro ni ejecución independiente de jobs",
            "El header menciona jobs",
            "Sin contrato ni flujo conectado",
            FuncionalidadEstado::Visual,
        ),
        FuncionalidadEntrada::new(
            "herdr",
            "No existe crate de integración en el workspace actual",
            "Sin modo host/socket integrado",
            "Pendiente de implementar",
            FuncionalidadEstado::NoImplementado,
        ),
        FuncionalidadEntrada::new(
            "MCP, skills y memoria",
            "Hay specs; no están conectados al loop actual",
            "Sin gestión visual funcional",
            "Pendiente conectar descubrimiento/contexto con cada step",
            FuncionalidadEstado::NoImplementado,
        ),
        FuncionalidadEntrada::new(
            "Voz, compresión, routing y KPIs",
            "Opciones documentadas, fuera del MVP actual",
            "Sin controles funcionales para esas capacidades",
            "Requerirán módulos/eventos propios si se priorizan",
            FuncionalidadEstado::NoImplementado,
        ),
    ]
}
