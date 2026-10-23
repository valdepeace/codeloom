//! Borde de A2A: traducción de los tipos del núcleo a los nombres del cable v1.0.
//!
//! Cabe señalar que este crate no abre sockets ni conoce el transporte; solo fija los nombres.

pub mod card;
pub mod client;
pub mod config;
pub mod remote;
pub mod security;
pub mod sync;
pub mod wire;
