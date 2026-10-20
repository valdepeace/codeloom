//! Perfil Self-model de Mindfork desde `self-model-dark-en.png`.

use crate::demo::Demo;
use batten::widgets::profile_view::{self, ProfileGoal, ProfileObservation};
use batten::widgets::Theme;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Block;
use ratatui::Frame;

pub struct MindforkSelfModelDemo;

impl Demo for MindforkSelfModelDemo {
    fn title(&self) -> &str {
        "Mindfork · self-model"
    }

    fn hint(&self) -> &str {
        "Assistant · User · goals · observations"
    }

    fn fullscreen(&self) -> bool {
        true
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        frame.render_widget(
            Block::default().style(Style::default().bg(ratatui::style::Color::Rgb(15, 16, 19))),
            area,
        );
        profile_view::render_mindfork(
            frame,
            area,
            "I run locally and help with practical engineering — model sizing, Rust, and the occasional trip plan. I prefer measured numbers to adjectives, and I write things down: advice should start from remembered facts, not fresh guesses. The current thread is fitting Gemma 4 onto a 12 GB card without giving up context; the next experiment on the list is a 1B draft for speculative decoding.",
            &[
                ProfileGoal { text: "Answer sizing questions from saved notes alone — no re-asking about hardware.", date: "2026-07-22", done: false },
                ProfileGoal { text: "Keep recommendations reproducible: name the exact quant, context and flags.", date: "2026-07-25", done: false },
                ProfileGoal { text: "Measure the 1B draft's speedup and save the number, not the impression.", date: "2026-07-31", done: false },
                ProfileGoal { text: "Index the llama.cpp server docs into the knowledge base.", date: "2026-07-19", done: true },
                ProfileGoal { text: "Turn the October trip research into a packing-checklist note.", date: "2026-07-21", done: true },
            ],
            "methodical, impatient with vague answers, allergic to hand-waving",
            "local model tuning, terminal tooling, speculative decoding",
            "Collaborative and direct; jokes land better after the numbers.",
            &[
                ProfileObservation { date: "2026-08-01", text: "Showing the exact command I ran turns out to be the fastest way to build trust." },
                ProfileObservation { date: "2026-07-30", text: "Answers that cite the note they came from get fewer follow-ups — receipts beat repetition." },
                ProfileObservation { date: "2026-07-28", text: "The 12 GB VRAM budget keeps coming up — saved it as a note so I stop re-asking." },
            ],
        );
    }

    fn handle_key(&mut self, _code: KeyCode) -> bool {
        false
    }
}
