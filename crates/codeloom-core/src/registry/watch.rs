//! Vigilancia de los directorios de agentes: recarga en caliente sin bloquear la interfaz.
//!
//! Un hilo dedicado sondea la fecha y el tamaño de los `*.md` de los directorios cada
//! `INTERVALO_SONDEO`. Solo recarga cuando la firma se mantiene estable durante un intervalo
//! completo (*debounce*), así que una ráfaga de guardados produce una recarga, no una por
//! escritura. Sondear en lugar de usar `notify` evita una dependencia nueva y cubre también
//! el directorio que aún no existe.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};

use crate::event::EventBus;
use crate::registry::AgentRegistry;

/// Intervalo de sondeo. Es también la ventana de *debounce*: una firma debe mantenerse
/// estable un intervalo entero antes de recargar.
const INTERVALO_SONDEO: Duration = Duration::from_millis(250);

/// Firma de los `*.md` de los directorios: ruta, fecha de modificación y tamaño.
type Firma = BTreeMap<PathBuf, (Option<SystemTime>, u64)>;

/// Vigilancia activa. Al soltarla, el hilo de sondeo termina antes de que `drop` retorne.
pub struct WatcherHandle {
    parar: Option<Sender<()>>,
    hilo: Option<JoinHandle<()>>,
}

impl Drop for WatcherHandle {
    fn drop(&mut self) {
        // Cerrar el canal despierta al hilo de inmediato; después se espera a que termine.
        self.parar.take();
        if let Some(hilo) = self.hilo.take() {
            let _ = hilo.join();
        }
    }
}

impl AgentRegistry {
    /// Arranca la vigilancia de `dirs`. Cada recarga con diferencias publica
    /// `AgentRegistryChanged` en `bus`.
    pub fn spawn_watcher(self: &Arc<Self>, dirs: Vec<PathBuf>, bus: EventBus) -> WatcherHandle {
        let (parar, recibir_parada) = mpsc::channel();
        let registro = Arc::clone(self);
        let hilo = std::thread::Builder::new()
            .name("codeloom-agent-watcher".to_string())
            .spawn(move || bucle(registro, bus, dirs, recibir_parada))
            .expect("no se pudo crear el hilo de vigilancia de agentes");
        WatcherHandle {
            parar: Some(parar),
            hilo: Some(hilo),
        }
    }
}

/// Bucle del hilo de vigilancia. Sale cuando el canal de parada se cierra.
fn bucle(registro: Arc<AgentRegistry>, bus: EventBus, dirs: Vec<PathBuf>, parar: Receiver<()>) {
    let mut ultima_vista: Option<Firma> = None;
    let mut ultima_recarga: Option<Firma> = None;
    loop {
        match parar.recv_timeout(INTERVALO_SONDEO) {
            Err(RecvTimeoutError::Timeout) => {}
            Ok(()) | Err(RecvTimeoutError::Disconnected) => return,
        }

        let actual = firma(&dirs);
        if ultima_vista.as_ref() != Some(&actual) {
            // Cambió algo: se espera a ver si la firma se estabiliza.
            ultima_vista = Some(actual);
            continue;
        }
        // Estable durante un intervalo completo y distinta de la última recarga.
        if ultima_recarga.as_ref() != Some(&actual) {
            registro.reload_and_publish(&bus);
            ultima_recarga = Some(actual);
        }
    }
}

/// Firma de los `*.md` de los directorios. Un directorio inexistente aporta nada.
fn firma(dirs: &[PathBuf]) -> Firma {
    let mut firma = Firma::new();
    for dir in dirs {
        let Ok(lectura) = std::fs::read_dir(dir) else {
            continue;
        };
        for dirent in lectura.flatten() {
            let ruta = dirent.path();
            if ruta.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            if let Ok(meta) = dirent.metadata() {
                if meta.is_file() {
                    firma.insert(ruta, (meta.modified().ok(), meta.len()));
                }
            }
        }
    }
    firma
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::LiveEvent;
    use crate::registry::{BuiltInSource, FileSource, ProfileLevel};
    use std::time::Duration;

    const ESPERA_MAX: Duration = Duration::from_secs(5);

    fn registro_sobre(dir: &std::path::Path) -> Arc<AgentRegistry> {
        Arc::new(AgentRegistry::new(vec![
            Box::new(BuiltInSource),
            Box::new(FileSource::new(dir.to_path_buf(), ProfileLevel::Global)),
        ]))
    }

    #[tokio::test]
    async fn watcher_picks_up_a_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let registro = registro_sobre(dir.path());
        let bus = EventBus::new(32);
        let mut rx = bus.subscribe();
        let _vigilancia = registro.spawn_watcher(vec![dir.path().to_path_buf()], bus.clone());

        std::fs::write(
            dir.path().join("nuevo.md"),
            "---\nname: nuevo\ndescription: llega en caliente\n---\n\nCuerpo.\n",
        )
        .unwrap();

        // Espera acotada: se sondea el evento hasta que aparezca el agente nuevo.
        let llegado = tokio::time::timeout(ESPERA_MAX, async {
            loop {
                match rx.recv().await {
                    Ok(LiveEvent::AgentRegistryChanged { added, .. })
                        if added.contains(&"nuevo".to_string()) =>
                    {
                        return true;
                    }
                    Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return false,
                }
            }
        })
        .await;

        assert_eq!(
            llegado,
            Ok(true),
            "no llegó AgentRegistryChanged con `nuevo`"
        );
        assert!(registro.snapshot().get("nuevo").is_some());
    }

    #[test]
    fn dropping_the_watcher_stops_it() {
        let dir = tempfile::tempdir().unwrap();
        let registro = registro_sobre(dir.path());
        let vigilancia = registro.spawn_watcher(vec![dir.path().to_path_buf()], EventBus::new(8));
        // El hilo conserva una copia del registro mientras vive.
        assert_eq!(Arc::strong_count(&registro), 2);

        drop(vigilancia);

        // Al soltarla, el hilo ya ha terminado y ha liberado su copia.
        assert_eq!(Arc::strong_count(&registro), 1);
    }
}
