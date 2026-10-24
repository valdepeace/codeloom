//! Borde de A2A: traducción de los tipos del núcleo a los nombres del cable v1.0.
//!
//! Cabe señalar que además de fijar los nombres del cable, este crate habla el transporte: el
//! cliente (`client`, A8a/A8d) llama a agentes remotos y el servidor (`serve`, A8b) abre un puerto,
//! solo en bucle local y solo con autenticación, para que otros clientes A2A usen un agente local.

pub mod card;
pub mod client;
pub mod config;
pub mod oauth;
pub mod remote;
pub mod security;
pub mod serve;
pub mod sse;
pub mod sync;
pub mod wire;
