//! Composición de trabajo completa con las primitivas disponibles en Batten.

use crate::demo::Demo;
use batten::state::{
    Composer, FileChange, FileChanges, Format, LineKind, Sidebar, Tone, ToolItem, Transcript,
    Viewport,
};
use batten::widgets::{
    composer, file_changes,
    matrix_rain::MatrixRain,
    panel, progress,
    queue::{self, QueueEntry, QueueStatus},
    sidebar, status,
    status_bar::{self, PawanStatus},
    thinking, tool_item, transcript, Anim, Panel, Theme,
};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct DashboardDemo {
    transcript: Transcript,
    viewport: Viewport,
    transcript_layout: transcript::Layout,
    composer: Composer,
    sidebar: Sidebar,
    tool: ToolItem,
    changes: FileChanges,
    rain: MatrixRain,
    rain_on: bool,
    anim: Anim,
}

impl DashboardDemo {
    pub fn new() -> Self {
        let mut transcript = Transcript::new();
        transcript.push_entry(
            LineKind::User,
            Format::Plain,
            "Revisa el flujo de MatrixRain y termina el dashboard visual.",
        );
        transcript.push_entry(
            LineKind::Assistant,
            Format::Markdown,
            "He localizado el ciclo de animación.\n\n- El fondo emite gotas durante el turno.\n- Al terminar, las gotas activas se drenan.\n- Los widgets siguen pintándose por encima.",
        );
        let mut sidebar = Sidebar::with_logo("batten");
        sidebar.push_section("AGENT", vec!["● working".into(), "claude-sonnet-4".into()]);
        sidebar.push_section("PROJECT", vec!["loom".into(), "src/widgets".into()]);
        sidebar.push_section("FLOCK", vec!["● 2 running".into(), "○ 1 queued".into()]);
        sidebar.push_section("SESSION", vec!["38.4k / 200k tok".into()]);

        let mut tool = ToolItem::new("call-42", "edit");
        tool.start();
        tool.set_summary("actualizando el renderer del dashboard");

        let mut changes = FileChanges::new();
        changes.upsert(FileChange {
            path: "crates/batten/src/widgets/matrix_rain.rs".into(),
            added: 42,
            removed: 3,
        });
        changes.upsert(FileChange {
            path: "crates/codeloom-tui/src/render.rs".into(),
            added: 5,
            removed: 0,
        });
        changes.upsert(FileChange {
            path: "README.md".into(),
            added: 8,
            removed: 2,
        });

        let mut composer = Composer::new();
        composer.set_text("Añade un estado de drenaje suave al MatrixRain.");
        Self {
            transcript,
            viewport: Viewport::new(),
            transcript_layout: transcript::Layout::new(),
            composer,
            sidebar,
            tool,
            changes,
            rain: MatrixRain::default(),
            rain_on: false,
            anim: Anim::new(),
        }
    }

    fn render_center(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let inner = panel::render(
            frame,
            area,
            &Panel {
                title: "Conversation",
                info: "run #238 · working",
                tone: Tone::Info,
            },
            theme,
        );
        if inner.is_empty() {
            return;
        }
        let rows = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(4),
            Constraint::Length(1),
            Constraint::Length(5),
            Constraint::Length(1),
        ])
        .split(inner);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("run 238", theme.styles.heading.add_modifier(Modifier::BOLD)),
                Span::styled("  ·  refactor matrix background", theme.styles.muted),
            ])),
            rows[0],
        );
        transcript::render(
            frame,
            rows[1],
            &self.transcript,
            &mut self.viewport,
            &mut self.transcript_layout,
            None,
            theme,
        );
        tool_item::render(frame, rows[2], &self.tool, &self.anim, theme);
        thinking::render(
            frame,
            rows[3],
            "Thinking through the shutdown path…",
            &self.anim,
            theme,
        );
        composer::render_focused(frame, rows[4], &self.composer, false, theme);
        frame.render_widget(
            Paragraph::new(batten::widgets::button_row(
                &[
                    ("Explain", Tone::Info),
                    ("Apply", Tone::Success),
                    ("Cancel", Tone::Error),
                ],
                1,
                theme,
            )),
            rows[5],
        );
    }

    fn render_metrics(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let rows = Layout::vertical([
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(7),
            Constraint::Min(1),
        ])
        .split(area);
        let run_inner = panel::render(
            frame,
            rows[0],
            &Panel {
                title: "Agent",
                info: "active",
                tone: Tone::Success,
            },
            theme,
        );
        frame.render_widget(
            Paragraph::new(vec![
                status(Tone::Success, "Working", "step 4/7", run_inner.width, theme),
                progress::progress_smooth(0.58, run_inner.width, theme),
            ]),
            run_inner,
        );

        let context_inner = panel::render(
            frame,
            rows[1],
            &Panel {
                title: "Context",
                info: "38.4k / 200k",
                tone: Tone::Neutral,
            },
            theme,
        );
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled("Claude Sonnet 4", theme.styles.default)),
                progress::progress(0.19, context_inner.width, theme),
            ]),
            context_inner,
        );

        let queue_inner = panel::render(
            frame,
            rows[2],
            &Panel {
                title: "Flock queue",
                info: "2 active",
                tone: Tone::Info,
            },
            theme,
        );
        queue::render_pawan(
            frame,
            queue_inner,
            &[
                QueueEntry {
                    label: "inspect UI",
                    status: QueueStatus::Running,
                },
                QueueEntry {
                    label: "run checks",
                    status: QueueStatus::Pending,
                },
                QueueEntry {
                    label: "scan files",
                    status: QueueStatus::Done,
                },
            ],
            self.anim.frame,
        );

        let files_inner = panel::render(
            frame,
            rows[3],
            &Panel {
                title: "Changed files",
                info: "3",
                tone: Tone::Neutral,
            },
            theme,
        );
        file_changes::render(frame, files_inner, "diff", &self.changes, theme);
    }
}

impl Demo for DashboardDemo {
    fn title(&self) -> &str {
        "Batten dashboard"
    }

    fn hint(&self) -> &str {
        "m: alternar MatrixRain de fondo · x: expandir tool"
    }

    fn fullscreen(&self) -> bool {
        true
    }

    fn tick(&mut self, anim: &Anim) {
        self.anim = *anim;
        self.rain.advance();
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if self.rain_on {
            self.rain.render(frame, area);
        }
        let rows = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled("  BATTEN / WORKSPACE", theme.styles.heading.add_modifier(Modifier::BOLD)),
                    Span::raw("  "),
                    batten::widgets::chip("WORKING", Tone::Success, theme),
                    Span::styled("   agent → flock → jobs", theme.styles.muted),
                ]),
                Line::from(Span::styled(
                    "  conversation · execution · context · files  |  m: MatrixRain  x: expand tool",
                    theme.styles.muted,
                )),
            ]),
            rows[0],
        );

        let wide = rows[1].width >= 105;
        let (left, center, right) = if wide {
            let [left, center, right] = Layout::horizontal([
                Constraint::Length(23),
                Constraint::Min(42),
                Constraint::Length(30),
            ])
            .areas(rows[1]);
            (Some(left), center, Some(right))
        } else if rows[1].width >= 68 {
            let [center, right] =
                Layout::horizontal([Constraint::Min(1), Constraint::Length(28)]).areas(rows[1]);
            (None, center, Some(right))
        } else {
            (None, rows[1], None)
        };
        if let Some(left) = left {
            sidebar::render(frame, left, &self.sidebar, theme);
        }
        self.render_center(frame, center, theme);
        if let Some(right) = right {
            self.render_metrics(frame, right, theme);
        }
        status_bar::render_pawan(
            frame,
            rows[2],
            &PawanStatus {
                mode: "build",
                thinking: Some("working"),
                branch: Some("matrix-rain"),
                goal_active: true,
                loop_active: false,
                model: "claude-sonnet-4",
                total_tokens: 38_420,
                context_pct: 0.19,
                iteration: 4,
                timestamp: "12s",
            },
        );
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char('m') => {
                self.rain_on = !self.rain_on;
                if self.rain_on {
                    self.rain.start();
                } else {
                    self.rain.stop();
                }
                true
            }
            KeyCode::Char('x') => {
                self.tool.toggle_expanded();
                true
            }
            _ => false,
        }
    }
}
