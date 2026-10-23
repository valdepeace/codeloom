//! Servir CodeLoom como agente A2A (A8b).
//!
//! Cabe señalar que servir invierte el riesgo de consumir: es un tercero quien dicta a un modelo
//! local lo que ha de hacer, sin persona que apruebe permisos. Por eso la configuración
//! (`config`) valida primero que haya autenticación y que se escuche solo en bucle local, y el
//! resto del servidor se construye sobre esa base.

pub mod auth;
pub mod card;
pub mod config;
