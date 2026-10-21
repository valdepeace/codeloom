//! Catálogo de proveedores y modelos obtenido de models.dev.
//!
//! Se sirve de un JSON de ejemplo embebido en las pruebas: ningún test toca la red.

use crate::config::ProviderKind;
use serde::Deserialize;
use std::collections::HashMap;
use std::future::Future;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Dirección pública del catálogo de models.dev.
pub const CATALOG_URL: &str = "https://models.dev/api.json";

/// Tiempo que una copia en caché se considera vigente: veinticuatro horas.
pub const CATALOG_TTL: Duration = Duration::from_secs(24 * 3600);

/// Tiempo límite de la descarga del catálogo.
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);

/// Capacidades de razonamiento de un modelo, según `reasoning_options` de models.dev.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Reasoning {
    /// Niveles con nombre (`low`, `high`…) de la opción `effort`, en el orden del catálogo.
    pub efforts: Vec<String>,
    /// Rango `(mínimo, máximo)` de la opción `budget_tokens`, si la publica.
    pub budget: Option<(Option<u64>, Option<u64>)>,
    /// Indica si el modelo admite activar o desactivar el razonamiento (`toggle`).
    pub toggle: bool,
}

/// Modelo del catálogo, con solo lo que CodeLoom puede aprovechar.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogModel {
    pub id: String,
    pub name: String,
    /// Ventana de contexto en tokens, si el catálogo la publica.
    pub context: Option<u64>,
    /// Máximo de tokens de salida, si el catálogo lo publica.
    pub max_output: Option<u64>,
    /// Coste de entrada por millón de tokens.
    pub input_cost: Option<f64>,
    /// Coste de salida por millón de tokens.
    pub output_cost: Option<f64>,
    /// Capacidades de razonamiento; `None` si el modelo no razona.
    pub reasoning: Option<Reasoning>,
    /// Fecha de publicación en formato `AAAA-MM-DD`.
    pub release_date: Option<String>,
}

/// Proveedor del catálogo, ya traducido a un tipo de SDK soportado.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogProvider {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: String,
    /// Primera variable de `env` del proveedor, o cadena vacía si no la hay.
    pub api_key_env: String,
    pub models: Vec<CatalogModel>,
}

/// Catálogo completo, listo para la interfaz.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Catalog {
    pub providers: Vec<CatalogProvider>,
}

/// Errores del catálogo: parseo, caché en disco o descarga.
#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("el catálogo no es un JSON válido: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("error de E/S en el catálogo: {0}")]
    Io(#[from] std::io::Error),
    #[error("error al descargar el catálogo: {0}")]
    Http(#[from] reqwest::Error),
}

// Formas crudas del JSON de models.dev. Todo lo opcional se tolera ausente.

#[derive(Deserialize)]
struct RawProvider {
    id: Option<String>,
    name: Option<String>,
    npm: Option<String>,
    api: Option<String>,
    #[serde(default)]
    env: Vec<String>,
    #[serde(default)]
    models: HashMap<String, RawModel>,
}

#[derive(Deserialize)]
struct RawModel {
    id: Option<String>,
    name: Option<String>,
    tool_call: Option<bool>,
    reasoning: Option<bool>,
    reasoning_options: Option<Vec<RawReasoningOption>>,
    status: Option<String>,
    release_date: Option<String>,
    limit: Option<RawLimit>,
    cost: Option<RawCost>,
}

#[derive(Deserialize)]
struct RawReasoningOption {
    #[serde(rename = "type")]
    kind: Option<String>,
    values: Option<Vec<String>>,
    min: Option<u64>,
    max: Option<u64>,
}

#[derive(Deserialize)]
struct RawLimit {
    context: Option<u64>,
    output: Option<u64>,
}

#[derive(Deserialize)]
struct RawCost {
    input: Option<f64>,
    output: Option<f64>,
}

impl Catalog {
    /// Construye el catálogo a partir del JSON de models.dev, aplicando la traducción de
    /// `npm`, los filtros y el orden de presentación.
    pub fn from_json(raw: &str) -> Result<Catalog, CatalogError> {
        let parsed: HashMap<String, RawProvider> = serde_json::from_str(raw)?;
        let mut providers: Vec<CatalogProvider> = parsed
            .into_iter()
            .filter_map(|(key, provider)| build_provider(key, provider))
            .collect();
        providers.sort_by(|a, b| {
            provider_rank(&a.id)
                .cmp(&provider_rank(&b.id))
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(Catalog { providers })
    }

    /// Busca un proveedor por su identificador.
    pub fn provider(&self, id: &str) -> Option<&CatalogProvider> {
        self.providers.iter().find(|p| p.id == id)
    }

    /// Indica si el catálogo no tiene ningún proveedor utilizable.
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}

/// Traduce el paquete `npm` de models.dev al tipo de proveedor de CodeLoom.
fn translate_npm(npm: &str) -> Option<ProviderKind> {
    match npm {
        "@ai-sdk/anthropic" => Some(ProviderKind::Anthropic),
        "@ai-sdk/openai-compatible" | "@ai-sdk/openai" | "@openrouter/ai-sdk-provider" => {
            Some(ProviderKind::OpenAiCompat)
        }
        _ => None,
    }
}

/// URL base del proveedor: la que publica el catálogo o, si falta, la pública por defecto
/// de OpenAI y Anthropic. Cualquier otro proveedor sin `api` se descarta.
fn base_url(id: &str, api: Option<&str>) -> Option<String> {
    if let Some(api) = api.filter(|a| !a.trim().is_empty()) {
        return Some(api.trim().to_string());
    }
    match id {
        "openai" => Some("https://api.openai.com/v1".to_string()),
        "anthropic" => Some("https://api.anthropic.com".to_string()),
        _ => None,
    }
}

/// Posición de un proveedor en el orden de presentación: los frecuentes primero.
fn provider_rank(id: &str) -> u8 {
    match id {
        "openrouter" => 0,
        "anthropic" => 1,
        "openai" => 2,
        _ => 3,
    }
}

fn build_provider(key: String, raw: RawProvider) -> Option<CatalogProvider> {
    let kind = translate_npm(raw.npm.as_deref()?)?;
    let id = raw.id.unwrap_or(key);
    let base_url = base_url(&id, raw.api.as_deref())?;

    let mut models: Vec<CatalogModel> = raw
        .models
        .into_iter()
        .filter_map(|(key, model)| build_model(key, model))
        .collect();
    if models.is_empty() {
        return None;
    }
    // Más recientes primero; los que no tienen fecha, al final.
    models.sort_by(|a, b| {
        b.release_date
            .cmp(&a.release_date)
            .then_with(|| a.id.cmp(&b.id))
    });

    let name = raw.name.unwrap_or_else(|| id.clone());
    let api_key_env = raw.env.into_iter().next().unwrap_or_default();
    Some(CatalogProvider {
        id,
        name,
        kind,
        base_url,
        api_key_env,
        models,
    })
}

fn build_model(key: String, raw: RawModel) -> Option<CatalogModel> {
    if raw.tool_call != Some(true) || raw.status.as_deref() == Some("deprecated") {
        return None;
    }
    let id = raw.id.unwrap_or(key);
    let name = raw.name.unwrap_or_else(|| id.clone());
    let (context, max_output) = raw.limit.map_or((None, None), |l| (l.context, l.output));
    let (input_cost, output_cost) = raw.cost.map_or((None, None), |c| (c.input, c.output));
    Some(CatalogModel {
        id,
        name,
        context,
        max_output,
        input_cost,
        output_cost,
        reasoning: build_reasoning(raw.reasoning, raw.reasoning_options),
        release_date: raw.release_date,
    })
}

/// Traduce `reasoning_options` a `Reasoning`. Sin `reasoning` verdadero el modelo no razona
/// (`None`); con `reasoning` verdadero pero sin opciones, queda el valor por defecto.
fn build_reasoning(
    reasoning: Option<bool>,
    options: Option<Vec<RawReasoningOption>>,
) -> Option<Reasoning> {
    if reasoning != Some(true) {
        return None;
    }
    let options = options.unwrap_or_default();
    let find = |kind: &str| options.iter().find(|o| o.kind.as_deref() == Some(kind));
    Some(Reasoning {
        efforts: find("effort")
            .and_then(|o| o.values.clone())
            .unwrap_or_default(),
        budget: find("budget_tokens").map(|o| (o.min, o.max)),
        toggle: find("toggle").is_some(),
    })
}

/// Ruta de la caché en disco (`~/.codeloom/models-dev.json`), o `None` si no hay directorio home.
pub fn cache_path() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE")
        .filter(|v| !v.is_empty())
        .or_else(|| std::env::var_os("HOME").filter(|v| !v.is_empty()))?;
    Some(
        PathBuf::from(home)
            .join(".codeloom")
            .join("models-dev.json"),
    )
}

/// Lee la caché: el catálogo y la fecha de su última modificación. `None` si falta o está corrupta.
pub fn read_cache(path: &Path) -> Option<(Catalog, SystemTime)> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let raw = std::fs::read_to_string(path).ok()?;
    let catalog = Catalog::from_json(&raw).ok()?;
    Some((catalog, modified))
}

/// Indica si una copia modificada en `modified` ya superó el TTL respecto a `now`.
pub fn is_stale(modified: SystemTime, now: SystemTime) -> bool {
    now.duration_since(modified)
        .is_ok_and(|age| age > CATALOG_TTL)
}

/// Escribe la caché en un fichero temporal y lo renombra sobre la ruta definitiva,
/// de modo que una lectura concurrente nunca vea un fichero a medias.
pub fn write_cache_atomic(path: &Path, raw: &str) -> Result<(), CatalogError> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    match write_then_rename(&tmp, path, raw) {
        Ok(()) => Ok(()),
        Err(err) => {
            // Limpieza best-effort del temporal; el error original es el que importa.
            let _ = std::fs::remove_file(&tmp);
            Err(err.into())
        }
    }
}

fn write_then_rename(tmp: &Path, path: &Path, raw: &str) -> std::io::Result<()> {
    let mut file = std::fs::File::create(tmp)?;
    file.write_all(raw.as_bytes())?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(tmp, path)
}

/// Descarga el catálogo en crudo con un tiempo límite de quince segundos.
pub async fn fetch_raw(url: &str) -> Result<String, CatalogError> {
    let client = reqwest::Client::builder().timeout(FETCH_TIMEOUT).build()?;
    let body = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    Ok(body)
}

/// Obtiene el catálogo con `fetch`, lo valida con `from_json` y solo entonces actualiza la caché.
/// Un fallo al escribir la caché se registra, pero no impide devolver el catálogo recién obtenido.
pub async fn refresh<F, Fut>(path: &Path, fetch: F) -> Result<Catalog, CatalogError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<String, CatalogError>>,
{
    let raw = fetch().await?;
    let catalog = Catalog::from_json(&raw)?;
    if let Err(err) = write_cache_atomic(path, &raw) {
        tracing::warn!(error = %err, "no se pudo escribir la caché del catálogo");
    }
    Ok(catalog)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// JSON de ejemplo con tres proveedores soportados y uno no soportado.
    ///
    /// - `anthropic`: sin `api` (debe salir la URL por defecto).
    /// - `openrouter`: un modelo sin `tool_call` y otro `deprecated`.
    /// - `weird`: `npm` no soportado; se descarta entero.
    /// - `openai`, `zeta-llm`, `alpha-llm`: para comprobar el orden.
    const SAMPLE: &str = r#"{
        "anthropic": {
            "id": "anthropic", "name": "Anthropic", "npm": "@ai-sdk/anthropic",
            "env": ["ANTHROPIC_API_KEY"],
            "models": {
                "claude-a": {"id": "claude-a", "name": "Claude A", "tool_call": true,
                    "reasoning": true, "release_date": "2025-01-01",
                    "limit": {"context": 200000, "output": 8192},
                    "cost": {"input": 3, "output": 15}},
                "claude-b": {"id": "claude-b", "name": "Claude B", "tool_call": true,
                    "release_date": "2026-02-04",
                    "limit": {"context": 1000000, "output": 128000},
                    "cost": {"input": 5, "output": 25}},
                "claude-no-tools": {"id": "claude-no-tools", "name": "Sin herramientas",
                    "tool_call": false, "release_date": "2026-03-01"}
            }
        },
        "openrouter": {
            "id": "openrouter", "name": "OpenRouter", "npm": "@openrouter/ai-sdk-provider",
            "api": "https://openrouter.ai/api/v1", "env": ["OPENROUTER_API_KEY"],
            "models": {
                "m-ok": {"id": "m-ok", "name": "Modelo OK", "tool_call": true},
                "m-no-tools": {"id": "m-no-tools", "name": "Sin tools", "tool_call": false},
                "m-old": {"id": "m-old", "name": "Viejo", "tool_call": true,
                    "status": "deprecated"}
            }
        },
        "weird": {
            "id": "weird", "name": "Weird", "npm": "@ai-sdk/google",
            "api": "https://weird.example/v1", "env": ["WEIRD_KEY"],
            "models": {
                "w-1": {"id": "w-1", "name": "W1", "tool_call": true}
            }
        },
        "openai": {
            "id": "openai", "name": "OpenAI", "npm": "@ai-sdk/openai",
            "env": ["OPENAI_API_KEY"],
            "models": {
                "gpt-x": {"id": "gpt-x", "name": "GPT X", "tool_call": true,
                    "release_date": "2025-08-07"}
            }
        },
        "zeta-llm": {
            "id": "zeta-llm", "name": "Zeta", "npm": "@ai-sdk/openai-compatible",
            "api": "https://zeta.example/v1", "env": ["ZETA_KEY"],
            "models": {
                "z-1": {"id": "z-1", "name": "Z1", "tool_call": true}
            }
        },
        "alpha-llm": {
            "id": "alpha-llm", "name": "Alpha", "npm": "@ai-sdk/openai-compatible",
            "api": "https://alpha.example/v1", "env": ["ALPHA_KEY"],
            "models": {
                "a-1": {"id": "a-1", "name": "A1", "tool_call": true}
            }
        }
    }"#;

    fn temp_cache_path(dir: &tempfile::TempDir) -> std::path::PathBuf {
        dir.path().join("models-dev.json")
    }

    #[test]
    fn maps_npm_to_provider_kind() {
        let catalog = Catalog::from_json(SAMPLE).expect("el JSON de ejemplo es válido");

        let anthropic = catalog.provider("anthropic").expect("anthropic debe estar");
        assert_eq!(anthropic.kind, crate::config::ProviderKind::Anthropic);

        let openrouter = catalog
            .provider("openrouter")
            .expect("openrouter debe estar");
        assert_eq!(openrouter.kind, crate::config::ProviderKind::OpenAiCompat);
        assert_eq!(openrouter.base_url, "https://openrouter.ai/api/v1");
        assert_eq!(openrouter.api_key_env, "OPENROUTER_API_KEY");

        // Sin `api`, OpenAI usa su URL pública por defecto.
        let openai = catalog.provider("openai").expect("openai debe estar");
        assert_eq!(openai.kind, crate::config::ProviderKind::OpenAiCompat);
        assert_eq!(openai.base_url, "https://api.openai.com/v1");

        // Sin `api`, Anthropic usa su URL pública por defecto.
        assert_eq!(anthropic.base_url, "https://api.anthropic.com");
    }

    #[test]
    fn drops_unsupported_providers_and_models() {
        let catalog = Catalog::from_json(SAMPLE).expect("el JSON de ejemplo es válido");

        // `npm` no soportado: fuera entero.
        assert!(catalog.provider("weird").is_none());

        // Sin `tool_call` verdadero o marcado `deprecated`: fuera.
        let anthropic = catalog.provider("anthropic").expect("anthropic debe estar");
        let ids: Vec<&str> = anthropic.models.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, vec!["claude-b", "claude-a"]);

        let openrouter = catalog
            .provider("openrouter")
            .expect("openrouter debe estar");
        let ids: Vec<&str> = openrouter.models.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, vec!["m-ok"]);
    }

    #[test]
    fn orders_providers_and_models() {
        let catalog = Catalog::from_json(SAMPLE).expect("el JSON de ejemplo es válido");

        // Primero los frecuentes (openrouter, anthropic, openai); luego alfabético por nombre.
        let ids: Vec<&str> = catalog.providers.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["openrouter", "anthropic", "openai", "alpha-llm", "zeta-llm"]
        );

        // Modelos por fecha de publicación descendente; los que no tienen fecha, al final.
        let anthropic = catalog.provider("anthropic").expect("anthropic debe estar");
        assert_eq!(
            anthropic.models[0].release_date.as_deref(),
            Some("2026-02-04")
        );
        assert_eq!(
            anthropic.models[1].release_date.as_deref(),
            Some("2025-01-01")
        );

        let openrouter = catalog
            .provider("openrouter")
            .expect("openrouter debe estar");
        assert_eq!(openrouter.models.len(), 1);
    }

    #[test]
    fn tolerates_missing_fields() {
        // Sin `name`, `env`, `limit`, `cost`, `release_date` ni `reasoning`.
        let raw = r#"{
            "bare": {
                "id": "bare", "npm": "@ai-sdk/openai-compatible", "api": "https://bare.example/v1",
                "models": {
                    "m": {"id": "m", "tool_call": true}
                }
            }
        }"#;
        let catalog =
            Catalog::from_json(raw).expect("los campos ausentes no deben romper el parseo");

        let bare = catalog.provider("bare").expect("bare debe estar");
        assert_eq!(bare.name, "bare", "sin nombre se usa el id");
        assert_eq!(bare.api_key_env, "", "sin env la variable queda vacía");
        assert_eq!(bare.models.len(), 1);

        let model = &bare.models[0];
        assert_eq!(model.name, "m", "sin nombre se usa el id");
        assert_eq!(model.context, None);
        assert_eq!(model.max_output, None);
        assert_eq!(model.input_cost, None);
        assert_eq!(model.output_cost, None);
        assert!(model.reasoning.is_none());
        assert_eq!(model.release_date, None);
    }

    #[test]
    fn corrupt_json_is_an_error() {
        let result = Catalog::from_json("{ esto no es json");
        assert!(
            matches!(result, Err(CatalogError::Parse(_))),
            "debe ser Parse"
        );
    }

    #[test]
    fn stale_after_ttl() {
        let now = SystemTime::now();
        let ttl = CATALOG_TTL;

        assert!(
            is_stale(now - ttl - Duration::from_secs(1), now),
            "pasado el TTL caduca"
        );
        assert!(
            !is_stale(now - Duration::from_secs(3600), now),
            "una hora no caduca"
        );
        assert!(!is_stale(now, now), "recién escrito no caduca");
    }

    #[test]
    fn cache_roundtrip() {
        let dir = tempfile::tempdir().expect("directorio temporal");
        let path = temp_cache_path(&dir);

        write_cache_atomic(&path, SAMPLE).expect("escritura de caché");
        let (catalog, modified) = read_cache(&path).expect("la caché recién escrita se lee");

        assert_eq!(catalog.providers.len(), 5);
        assert!(!is_stale(modified, SystemTime::now()));

        // La escritura atómica no debe dejar ficheros temporales sueltos.
        let entries: Vec<String> = std::fs::read_dir(dir.path())
            .expect("leer directorio")
            .map(|e| {
                e.expect("entrada")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(entries, vec!["models-dev.json".to_string()]);
    }

    #[test]
    fn corrupt_cache_reads_as_none() {
        let dir = tempfile::tempdir().expect("directorio temporal");
        let path = temp_cache_path(&dir);

        assert!(read_cache(&path).is_none(), "si falta, None");

        std::fs::write(&path, "basura {{{").expect("escribir basura");
        assert!(read_cache(&path).is_none(), "si está corrupta, None");
    }

    #[tokio::test]
    async fn refresh_does_not_write_cache_on_bad_json() {
        let dir = tempfile::tempdir().expect("directorio temporal");
        let path = temp_cache_path(&dir);

        let result = refresh(&path, || async { Ok("no es json".to_string()) }).await;

        assert!(matches!(result, Err(CatalogError::Parse(_))));
        assert!(!path.exists(), "un JSON malo no debe sobrescribir la caché");
    }

    #[tokio::test]
    async fn refresh_writes_cache_on_good_json() {
        let dir = tempfile::tempdir().expect("directorio temporal");
        let path = temp_cache_path(&dir);

        let catalog = refresh(&path, || async { Ok(SAMPLE.to_string()) })
            .await
            .expect("el JSON de ejemplo debe refrescar el catálogo");

        assert_eq!(catalog.providers.len(), 5);
        let (cached, _) = read_cache(&path).expect("la caché debe haberse escrito");
        assert_eq!(cached.providers.len(), 5);
    }

    /// Catálogo mínimo con un único modelo de proveedor `p`, con el JSON del modelo dado.
    fn catalog_with_model(model_json: &str) -> Catalog {
        let raw = format!(
            r#"{{
                "p": {{
                    "id": "p", "npm": "@ai-sdk/openai-compatible", "api": "https://p.example/v1",
                    "models": {{ "m": {model_json} }}
                }}
            }}"#
        );
        Catalog::from_json(&raw).expect("el JSON de prueba es válido")
    }

    #[test]
    fn parses_effort_values() {
        let catalog = catalog_with_model(
            r#"{"id": "m", "tool_call": true, "reasoning": true,
                "reasoning_options": [{"type": "effort", "values": ["low", "medium", "high"]}]}"#,
        );
        let model = &catalog.provider("p").expect("p debe estar").models[0];
        let reasoning = model.reasoning.as_ref().expect("el modelo razona");
        assert_eq!(reasoning.efforts, ["low", "medium", "high"]);
        assert_eq!(reasoning.budget, None);
        assert!(!reasoning.toggle);
    }

    #[test]
    fn parses_budget_range() {
        let catalog = catalog_with_model(
            r#"{"id": "m", "tool_call": true, "reasoning": true,
                "reasoning_options": [{"type": "toggle"},
                    {"type": "budget_tokens", "min": 512, "max": 24576}]}"#,
        );
        let reasoning = catalog.provider("p").expect("p debe estar").models[0]
            .reasoning
            .clone()
            .expect("el modelo razona");
        assert_eq!(reasoning.budget, Some((Some(512), Some(24576))));
        assert!(reasoning.toggle);
        assert!(reasoning.efforts.is_empty());
    }

    #[test]
    fn non_reasoning_model_has_no_reasoning() {
        let catalog = catalog_with_model(
            r#"{"id": "m", "tool_call": true, "reasoning": false,
                "reasoning_options": [{"type": "effort", "values": ["low"]}]}"#,
        );
        let model = &catalog.provider("p").expect("p debe estar").models[0];
        assert!(
            model.reasoning.is_none(),
            "sin razonamiento no hay Reasoning"
        );
    }

    #[test]
    fn reasoning_true_without_options_is_some_default() {
        let catalog = catalog_with_model(r#"{"id": "m", "tool_call": true, "reasoning": true}"#);
        let model = &catalog.provider("p").expect("p debe estar").models[0];
        assert_eq!(model.reasoning, Some(Reasoning::default()));
    }
}
