//! Aviso de presupuesto de gasto de la sesión (sección `[budget]` de `codeloom.toml`).
//!
//! Función pura y sin estado: no detiene el turno ni modifica el comportamiento del modelo.
//! Solo determina qué umbrales cruza el acumulado de la sesión al apuntar una respuesta.

use crate::error::{CoreError, Result};
use crate::session::UsageRecord;
use serde::Deserialize;
use std::path::Path;

/// Porcentajes del presupuesto en los que se avisa, en orden ascendente.
pub const WARN_LEVELS_PERCENT: [u8; 2] = [80, 100];

/// Sección opcional `[budget]` de `codeloom.toml`. Sin ella, o con ambas claves ausentes,
/// no hay presupuesto ni aviso.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct BudgetConfig {
    /// Tokens de entrada más salida acumulados en la sesión.
    pub session_tokens: Option<u64>,
    /// Dólares acumulados en la sesión, según los precios conocidos.
    pub session_cost_usd: Option<f64>,
}

/// Forma del fichero a efectos de la sección `[budget]`; el resto de claves se ignora.
#[derive(Deserialize)]
struct FicheroConBudget {
    #[serde(default)]
    budget: BudgetConfig,
}

impl BudgetConfig {
    /// Lee la sección `[budget]` de un texto TOML. Su ausencia da una configuración vacía.
    /// Rechaza los límites no positivos o no finitos.
    pub fn from_toml(text: &str) -> Result<BudgetConfig> {
        let fichero: FicheroConBudget =
            toml::from_str(text).map_err(|e| CoreError::Config(e.to_string()))?;
        let config = fichero.budget;
        if config.session_tokens == Some(0) {
            return Err(CoreError::Config(
                "budget.session_tokens debe ser mayor que cero".into(),
            ));
        }
        if let Some(coste) = config.session_cost_usd {
            if !coste.is_finite() || coste <= 0.0 {
                return Err(CoreError::Config(
                    "budget.session_cost_usd debe ser un número finito mayor que cero".into(),
                ));
            }
        }
        Ok(config)
    }

    pub fn load(path: &Path) -> Result<BudgetConfig> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| CoreError::Config(format!("read {}: {e}", path.display())))?;
        Self::from_toml(&text)
    }

    /// `true` si ninguna de las dos claves está definida.
    pub fn is_empty(&self) -> bool {
        self.session_tokens.is_none() && self.session_cost_usd.is_none()
    }
}

/// Métrica del presupuesto que ha cruzado un umbral.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetMetric {
    Tokens,
    CostUsd,
}

/// Aviso emitido al cruzar un nivel del presupuesto.
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetWarning {
    pub metric: BudgetMetric,
    pub level_percent: u8,
    /// Acumulado tras el apunte (tokens o dólares).
    pub used: f64,
    pub limit: f64,
    /// Solo `CostUsd`: `true` si algún apunte no tenía precio y, por tanto, no suma.
    pub partial: bool,
}

/// Avisos de los umbrales que cruza el acumulado de la sesión al añadir `added`.
///
/// `before` son los apuntes previos de la sesión. Un umbral avisa si `antes < umbral <= despues`,
/// de modo que cada nivel se comunica una sola vez por sesión sin guardar estado. Los avisos salen
/// en orden de métrica (tokens y luego coste) y de nivel ascendente.
pub fn budget_crossings(
    config: &BudgetConfig,
    before: &[UsageRecord],
    added: &UsageRecord,
) -> Vec<BudgetWarning> {
    let mut avisos = Vec::new();

    if let Some(limite) = config.session_tokens {
        let antes: f64 = before
            .iter()
            .map(|r| r.input_tokens.saturating_add(r.output_tokens) as f64)
            .sum();
        let anadido = added.input_tokens.saturating_add(added.output_tokens) as f64;
        push_crossings(
            &mut avisos,
            BudgetMetric::Tokens,
            antes,
            antes + anadido,
            limite as f64,
            false,
        );
    }

    if let Some(limite) = config.session_cost_usd {
        let mut antes = 0.0;
        let mut partial = false;
        for registro in before {
            match registro.cost() {
                Some(coste) => antes += coste,
                None => partial = true,
            }
        }
        let anadido = match added.cost() {
            Some(coste) => coste,
            None => {
                partial = true;
                0.0
            }
        };
        push_crossings(
            &mut avisos,
            BudgetMetric::CostUsd,
            antes,
            antes + anadido,
            limite,
            partial,
        );
    }

    avisos
}

fn push_crossings(
    avisos: &mut Vec<BudgetWarning>,
    metric: BudgetMetric,
    antes: f64,
    despues: f64,
    limit: f64,
    partial: bool,
) {
    for nivel in WARN_LEVELS_PERCENT {
        let umbral = limit * f64::from(nivel) / 100.0;
        if antes < umbral && umbral <= despues {
            avisos.push(BudgetWarning {
                metric,
                level_percent: nivel,
                used: despues,
                limit,
                partial,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::SessionId;

    fn registro(input: u64, output: u64, precios: Option<(f64, f64)>) -> UsageRecord {
        UsageRecord {
            session: SessionId(1),
            model: "m".into(),
            input_tokens: input,
            output_tokens: output,
            input_price: precios.map(|p| p.0),
            output_price: precios.map(|p| p.1),
            created_at_ms: 0,
        }
    }

    fn solo_tokens(limite: u64) -> BudgetConfig {
        BudgetConfig {
            session_tokens: Some(limite),
            session_cost_usd: None,
        }
    }

    fn solo_coste(limite: f64) -> BudgetConfig {
        BudgetConfig {
            session_tokens: None,
            session_cost_usd: Some(limite),
        }
    }

    #[test]
    fn from_toml_reads_the_budget_section() {
        let text = "model = \"gpt-4o\"\n[agents]\ntrust_project_agents = true\n\
                    [budget]\nsession_tokens = 500000\nsession_cost_usd = 2.5\n";
        let config = BudgetConfig::from_toml(text).unwrap();
        assert_eq!(
            config,
            BudgetConfig {
                session_tokens: Some(500_000),
                session_cost_usd: Some(2.5),
            }
        );
        assert!(!config.is_empty());

        let solo_una = BudgetConfig::from_toml("[budget]\nsession_tokens = 1000\n").unwrap();
        assert_eq!(solo_una.session_cost_usd, None);
        assert!(
            !solo_una.is_empty(),
            "una sola clave basta para tener presupuesto"
        );
    }

    #[test]
    fn from_toml_without_section_is_empty() {
        let sin_seccion = "model = \"gpt-4o\"\n[agents]\ntrust_project_agents = true\n";
        let config = BudgetConfig::from_toml(sin_seccion).unwrap();
        assert_eq!(config, BudgetConfig::default());
        assert!(config.is_empty(), "sin [budget] no hay presupuesto");
    }

    #[test]
    fn from_toml_rejects_zero_negative_and_nan_limits() {
        let casos = [
            "[budget]\nsession_tokens = 0\n",
            "[budget]\nsession_cost_usd = 0.0\n",
            "[budget]\nsession_cost_usd = -1.5\n",
            "[budget]\nsession_cost_usd = nan\n",
            "[budget]\nsession_cost_usd = inf\n",
            "[budget]\nsession_tokens = -5\n",
        ];
        for texto in casos {
            let error = BudgetConfig::from_toml(texto).unwrap_err();
            assert!(
                matches!(error, CoreError::Config(_)),
                "{texto:?} dio {error:?}"
            );
        }
    }

    #[test]
    fn crossing_80_percent_warns_once() {
        let config = solo_tokens(1000);
        let antes = [registro(790, 0, None)];
        let primero = budget_crossings(&config, &antes, &registro(20, 1, None));
        assert_eq!(primero.len(), 1, "79 % -> 81 % cruza solo el 80 %");
        assert_eq!(primero[0].metric, BudgetMetric::Tokens);
        assert_eq!(primero[0].level_percent, 80);
        assert_eq!(primero[0].used, 811.0);
        assert_eq!(primero[0].limit, 1000.0);
        assert!(!primero[0].partial);

        let despues = [registro(790, 0, None), registro(20, 1, None)];
        let segundo = budget_crossings(&config, &despues, &registro(39, 0, None));
        assert!(
            segundo.is_empty(),
            "81 % -> 85 % no cruza ningún umbral nuevo"
        );
    }

    #[test]
    fn a_jump_across_both_levels_emits_both() {
        let config = solo_tokens(1000);
        let avisos = budget_crossings(&config, &[registro(700, 0, None)], &registro(600, 0, None));
        let niveles: Vec<u8> = avisos.iter().map(|a| a.level_percent).collect();
        assert_eq!(niveles, vec![80, 100], "70 % -> 130 % emite 80 y luego 100");
        assert!(avisos.iter().all(|a| a.used == 1300.0));
    }

    #[test]
    fn exactly_reaching_the_threshold_counts() {
        let config = solo_tokens(1000);

        let llega = budget_crossings(&config, &[], &registro(800, 0, None));
        assert_eq!(llega.len(), 1, "llegar exactamente al 80 % avisa");
        assert_eq!(llega[0].level_percent, 80);

        let ya_estaba = budget_crossings(&config, &[registro(800, 0, None)], &registro(1, 0, None));
        assert!(
            ya_estaba.is_empty(),
            "estar exactamente en el umbral antes no vuelve a avisar"
        );

        let llega_al_limite =
            budget_crossings(&config, &[registro(999, 0, None)], &registro(1, 0, None));
        assert_eq!(llega_al_limite.len(), 1, "999 -> 1000 cruza el 100 %");
        assert_eq!(llega_al_limite[0].level_percent, 100);
    }

    #[test]
    fn cost_ignores_unpriced_records_and_marks_partial() {
        let config = solo_coste(10.0);
        // Apunte con precio: 1 M de entrada a 3 USD/M = 3 USD. Apunte sin precio: no suma.
        let antes = [
            registro(1_000_000, 0, Some((3.0, 15.0))),
            registro(9_000_000, 0, None),
        ];
        // 2 M de entrada a 3 USD/M = 6 USD: 3 -> 9 cruza el 80 % (8 USD) y no el 100 % (10 USD).
        let avisos = budget_crossings(&config, &antes, &registro(2_000_000, 0, Some((3.0, 15.0))));
        assert_eq!(avisos.len(), 1);
        assert_eq!(avisos[0].metric, BudgetMetric::CostUsd);
        assert_eq!(avisos[0].level_percent, 80);
        assert_eq!(avisos[0].used, 9.0, "el apunte sin precio no suma al coste");
        assert_eq!(avisos[0].limit, 10.0);
        assert!(
            avisos[0].partial,
            "un apunte sin precio marca el aviso como parcial"
        );

        // Un apunte añadido sin precio suma 0 y no puede cruzar ningún umbral.
        let sin_precio = budget_crossings(
            &solo_coste(4.0),
            &[registro(1_000_000, 0, Some((3.0, 15.0)))],
            &registro(1, 0, None),
        );
        assert!(
            sin_precio.is_empty(),
            "un apunte sin precio no cruza umbrales"
        );
    }

    #[test]
    fn empty_config_never_warns() {
        let antes = [registro(
            10_000_000,
            10_000_000,
            Some((1_000_000.0, 1_000_000.0)),
        )];
        let avisos = budget_crossings(
            &BudgetConfig::default(),
            &antes,
            &registro(10_000_000, 0, None),
        );
        assert!(avisos.is_empty());
    }

    #[test]
    fn tokens_and_cost_warn_independently() {
        let config = BudgetConfig {
            session_tokens: Some(1000),
            session_cost_usd: Some(10.0),
        };
        // Entrada gratuita y salida a 1 M USD/M: el coste en dólares es igual a los tokens de salida.
        let solo_entrada = registro(800, 0, Some((0.0, 1_000_000.0)));

        // 800 tokens: cruza el 80 % de tokens; el coste sigue en 0.
        let a = budget_crossings(&config, &[], &solo_entrada);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].metric, BudgetMetric::Tokens);
        assert_eq!(a[0].level_percent, 80);

        // 8 USD de salida: cruza el 80 % de coste; los tokens pasan a 808 y no cruzan nada nuevo.
        let antes_b = [solo_entrada.clone()];
        let b = budget_crossings(&config, &antes_b, &registro(0, 8, Some((0.0, 1_000_000.0))));
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].metric, BudgetMetric::CostUsd);
        assert_eq!(b[0].level_percent, 80);

        // 2 USD más: cruza el 100 % de coste (10 USD) y nada de tokens.
        let antes_c = [
            solo_entrada.clone(),
            registro(0, 8, Some((0.0, 1_000_000.0))),
        ];
        let c = budget_crossings(&config, &antes_c, &registro(0, 2, Some((0.0, 1_000_000.0))));
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].metric, BudgetMetric::CostUsd);
        assert_eq!(c[0].level_percent, 100);

        // Un mismo apunte que cruza ambas métricas: primero tokens, luego coste, por nivel.
        let juntos = budget_crossings(&config, &[], &registro(1000, 10, Some((0.0, 1_000_000.0))));
        let orden: Vec<(BudgetMetric, u8)> =
            juntos.iter().map(|a| (a.metric, a.level_percent)).collect();
        assert_eq!(
            orden,
            vec![
                (BudgetMetric::Tokens, 80),
                (BudgetMetric::Tokens, 100),
                (BudgetMetric::CostUsd, 80),
                (BudgetMetric::CostUsd, 100),
            ]
        );
    }
}
