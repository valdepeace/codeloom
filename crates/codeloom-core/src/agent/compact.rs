//! Resumen del historial con el modelo: funciones puras de la compactación.
//!
//! Aquí no hay llamadas al proveedor ni acceso al almacén. El módulo calcula cuánto ocupa el
//! contexto, dónde se corta, cómo se representa un resumen y qué mensajes forman la petición.
//! El registro de la sesión no se toca: solo cambia lo que se envía al modelo.
//!
//! Las cifras son decisiones propias de CodeLoom; véase
//! `docs/specs/2026-10-23-codeloom-context-summary-design.md`.

use crate::message::{ContentPart, Message, Role};

/// Fracción de la ventana del modelo a partir de la cual se compacta.
pub const COMPACT_RATIO: f64 = 0.80;

/// Tokens estimados que se conservan íntegros al final del historial.
pub const KEEP_TAIL_TOKENS: usize = 10_000;

/// Tope en bytes del texto de un resumen, sin contar la cabecera.
pub const MAX_SUMMARY_BYTES: usize = 16_000;

/// Tope de tokens de la respuesta de la petición de resumen.
pub const SUMMARY_MAX_TOKENS: u32 = 4096;

/// Compactaciones permitidas dentro de un mismo turno.
pub const MAX_COMPACTIONS_PER_TURN: usize = 3;

/// Inicio de la cabecera de un resumen; lo que sigue es el número de mensajes cubiertos.
const SUMMARY_PREFIX: &str = "[Context summary: first ";

/// Final de la cabecera de un resumen, antes del corchete de cierre.
const SUMMARY_SUFFIX: &str = " messages";

/// Estimación de tokens de un texto: bytes entre cuatro, redondeando hacia arriba.
pub fn estimate_text_tokens(text: &str) -> usize {
    text.len().div_ceil(4)
}

/// Estimación de un mensaje: suma sobre sus partes. El texto y el razonamiento cuentan por
/// contenido; las llamadas, por nombre y JSON de entrada; los resultados, por contenido; una
/// imagen cuenta su URL.
pub fn estimate_message_tokens(message: &Message) -> usize {
    message
        .parts
        .iter()
        .map(|part| match part {
            ContentPart::Text { text } => estimate_text_tokens(text),
            ContentPart::Reasoning { text, signature } => {
                estimate_text_tokens(text) + signature.as_deref().map_or(0, estimate_text_tokens)
            }
            ContentPart::ImageUrl { url } => estimate_text_tokens(url),
            ContentPart::ToolCall { name, input, .. } => {
                estimate_text_tokens(name) + estimate_text_tokens(&input.to_string())
            }
            ContentPart::ToolResult { content, .. } => estimate_text_tokens(content),
            ContentPart::Finish { .. } => 0,
        })
        .sum()
}

/// Construye el mensaje de resumen: rol `User` con una única parte de texto que empieza por la
/// cabecera `[Context summary: first {covers} messages]`.
pub fn summary_message(covers: usize, text: &str) -> Message {
    Message::new(
        Role::User,
        vec![ContentPart::Text {
            text: format!("{SUMMARY_PREFIX}{covers}{SUMMARY_SUFFIX}]\n{text}"),
        }],
    )
}

/// Devuelve `Some((covers, texto))` si el mensaje es un resumen y `None` en cualquier otro caso.
/// Es tolerante: una cabecera mal formada no se interpreta como resumen.
pub fn parse_summary(message: &Message) -> Option<(usize, String)> {
    if message.role != Role::User {
        return None;
    }
    // Un resumen es siempre una única parte de texto; cualquier otra forma no lo es.
    let [ContentPart::Text { text }] = message.parts.as_slice() else {
        return None;
    };
    let resto = text.strip_prefix(SUMMARY_PREFIX)?;
    let fin = resto.find(']')?;
    let covers = resto[..fin]
        .strip_suffix(SUMMARY_SUFFIX)?
        .parse::<usize>()
        .ok()?;
    let cuerpo = resto[fin + 1..].strip_prefix('\n')?;
    Some((covers, cuerpo.to_string()))
}

/// Indica si el mensaje es un resumen.
fn is_summary(message: &Message) -> bool {
    parse_summary(message).is_some()
}

/// Posiciones en `history` de los mensajes que forman la petición: primero el último resumen y,
/// detrás, los mensajes de posición mayor o igual que su `covers`, salvo cualquier resumen. Sin
/// resumen, son todos los mensajes.
pub fn effective_indices(history: &[Message]) -> Vec<usize> {
    let ultimo = history
        .iter()
        .enumerate()
        .rev()
        .find_map(|(i, message)| parse_summary(message).map(|(covers, _)| (i, covers)));
    match ultimo {
        None => (0..history.len()).collect(),
        Some((s, covers)) => {
            let mut indices = vec![s];
            indices.extend((covers..history.len()).filter(|&i| i != s && !is_summary(&history[i])));
            indices
        }
    }
}

/// Copia de los mensajes de `effective_indices`, en el mismo orden.
pub fn effective_history(history: &[Message]) -> Vec<Message> {
    effective_indices(history)
        .into_iter()
        .map(|i| history[i].clone())
        .collect()
}

/// Tamaño estimado del contexto de la próxima petición, en tokens.
///
/// Se ancla en el recuento real del proveedor: se toma el último asistente de la petición con
/// `usage`, siempre que sea posterior al último resumen, y se suma lo que vino después. Sin
/// ancla válida se estima todo, prompt de sistema incluido, a razón de bytes entre cuatro.
pub fn estimate_context_tokens(history: &[Message], system_bytes: usize) -> usize {
    let idx = effective_indices(history);
    let resumen = history.iter().rposition(is_summary);
    let ancla = idx.iter().enumerate().rev().find_map(|(k, &i)| {
        let message = &history[i];
        let usage = message.usage.as_ref()?;
        let valida = message.role == Role::Assistant && resumen.is_none_or(|s| i > s);
        valida.then_some((k, usage))
    });
    match ancla {
        Some((k, usage)) => {
            let medido = usage.input_tokens as usize + usage.output_tokens as usize;
            medido
                + idx[k + 1..]
                    .iter()
                    .map(|&i| estimate_message_tokens(&history[i]))
                    .sum::<usize>()
        }
        None => {
            system_bytes / 4
                + idx
                    .iter()
                    .map(|&i| estimate_message_tokens(&history[i]))
                    .sum::<usize>()
        }
    }
}

/// `true` si el contexto estimado alcanza `COMPACT_RATIO` de la ventana del modelo.
pub fn should_compact(history: &[Message], system_bytes: usize, window: u64) -> bool {
    estimate_context_tokens(history, system_bytes) as f64 >= window as f64 * COMPACT_RATIO
}

/// Posición cruda `N` desde la que se conserva la cola, u `None` si no hay nada que resumir.
///
/// La cola se forma desde el final hasta alcanzar `KEEP_TAIL_TOKENS`. Después se legaliza el
/// corte: la cola nunca empieza por un resultado de herramienta, y nunca se corta antes de un
/// resumen existente, porque lo que quedara antes del corte se perdería.
pub fn choose_cut(history: &[Message]) -> Option<usize> {
    let idx = effective_indices(history);
    if idx.is_empty() {
        return None;
    }

    // Presupuesto: el elemento en el que se alcanza el umbral marca el corte candidato.
    let mut acumulado = 0usize;
    let mut k = 0usize;
    for (j, &i) in idx.iter().enumerate().rev() {
        acumulado += estimate_message_tokens(&history[i]);
        if acumulado >= KEEP_TAIL_TOKENS {
            k = j;
            break;
        }
    }

    // Límite inferior: sin resumen, cualquier posición; con resumen, solo lo posterior a él. Si
    // no hay nada posterior al resumen, no hay nada nuevo que resumir.
    let minimo = match history.iter().rposition(is_summary) {
        None => 0,
        Some(s) => idx.iter().position(|&i| i > s)?,
    };
    let mut k = k.max(minimo);

    // Legalización: la cola no empieza por un resultado de herramienta, que debe llevar su
    // llamada delante.
    while k > minimo && history[idx[k]].role == Role::Tool {
        k -= 1;
    }
    if history[idx[k]].role == Role::Tool {
        return None;
    }

    // Si en `idx[..k]` solo hay resúmenes, no queda nada nuevo que resumir.
    if idx[..k].iter().all(|&i| is_summary(&history[i])) {
        return None;
    }
    Some(idx[k])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::Usage;

    #[test]
    fn summary_round_trips() {
        assert_eq!(
            parse_summary(&summary_message(7, "hola\nmundo")),
            Some((7, "hola\nmundo".to_string()))
        );
    }

    #[test]
    fn parse_summary_rejects_ordinary_and_malformed_messages() {
        assert_eq!(parse_summary(&Message::user("hola")), None);
        assert_eq!(
            parse_summary(&Message::user("[Context summary: first x messages]\nhola")),
            None
        );
        assert_eq!(
            parse_summary(&Message::user("[Context summary: first 3 messages")),
            None
        );
    }

    #[test]
    fn effective_history_without_summary_is_the_whole_history() {
        let history = vec![
            Message::user("uno"),
            Message::user("dos"),
            Message::user("tres"),
        ];
        assert_eq!(effective_history(&history), history);
    }

    #[test]
    fn effective_history_uses_the_last_summary_and_the_messages_from_covers() {
        let history = vec![
            Message::user("m0"),
            Message::user("m1"),
            Message::user("m2"),
            Message::user("m3"),
            summary_message(2, "resumen"),
            Message::user("m5"),
        ];
        let esperado = vec![
            history[4].clone(),
            history[2].clone(),
            history[3].clone(),
            history[5].clone(),
        ];
        assert_eq!(effective_history(&history), esperado);
    }

    #[test]
    fn effective_history_drops_older_summaries() {
        let history = vec![
            Message::user("m0"),
            summary_message(1, "uno"),
            Message::user("m2"),
            summary_message(2, "dos"),
            Message::user("m4"),
        ];
        let esperado = vec![history[3].clone(), history[2].clone(), history[4].clone()];
        assert_eq!(effective_history(&history), esperado);
    }

    #[test]
    fn estimate_uses_the_anchor_after_the_last_summary_only() {
        // Asistente con `usage` anterior al resumen: no sirve de ancla, se estima por bytes.
        let mut antes = Message::assistant(vec![ContentPart::Text {
            text: "b".repeat(400),
        }]);
        antes.usage = Some(Usage {
            input_tokens: 90_000,
            output_tokens: 10,
        });
        let resumen = summary_message(2, "r");
        let esperado = estimate_message_tokens(&resumen);
        let history = vec![Message::user("a".repeat(400)), antes, resumen];
        assert_eq!(estimate_context_tokens(&history, 0), esperado);

        // Asistente con `usage` posterior al resumen: sí es ancla.
        let mut despues = Message::assistant(vec![ContentPart::Text {
            text: "b".repeat(400),
        }]);
        despues.usage = Some(Usage {
            input_tokens: 3000,
            output_tokens: 200,
        });
        let history = vec![
            Message::user("a".repeat(400)),
            summary_message(1, "r"),
            despues,
            Message::user("c".repeat(400)),
        ];
        // Ancla 3000 + 200, más el mensaje de usuario posterior (400 bytes, 100 tokens).
        assert_eq!(estimate_context_tokens(&history, 0), 3300);
    }

    #[test]
    fn should_compact_respects_the_ratio() {
        // 3196 bytes son 799 tokens; 3200 bytes son 800.
        let bajo = vec![Message::user("x".repeat(3196))];
        let alto = vec![Message::user("x".repeat(3200))];
        assert!(!should_compact(&bajo, 0, 1000));
        assert!(should_compact(&alto, 0, 1000));
    }

    #[test]
    fn choose_cut_never_starts_the_tail_with_a_tool_result() {
        // Índices pares: resultados de herramienta; impares: asistente. Cada mensaje cuesta 1000
        // tokens. Desde el final, el umbral se alcanza en el índice 20, que es un resultado.
        let history: Vec<Message> = (0..30)
            .map(|i| {
                if i % 2 == 0 {
                    Message::new(
                        Role::Tool,
                        vec![ContentPart::ToolResult {
                            call_id: format!("c{i}"),
                            content: "x".repeat(4000),
                            is_error: false,
                        }],
                    )
                } else {
                    Message::assistant(vec![ContentPart::Text {
                        text: "t".repeat(4000),
                    }])
                }
            })
            .collect();

        let cut = choose_cut(&history).expect("hay historia que resumir");
        assert_eq!(cut, 19, "el corte retrocede hasta el asistente anterior");
        assert_eq!(history[cut].role, Role::Assistant);
    }

    #[test]
    fn choose_cut_is_none_when_everything_fits_in_the_tail() {
        let history = vec![
            Message::user("x".repeat(100)),
            Message::user("x".repeat(100)),
            Message::user("x".repeat(100)),
        ];
        assert_eq!(choose_cut(&history), None);
    }

    #[test]
    fn choose_cut_keeps_roughly_the_tail_budget() {
        // 100 mensajes de 4000 bytes: 1000 tokens cada uno.
        let history: Vec<Message> = (0..100).map(|_| Message::user("x".repeat(4000))).collect();
        let cut = choose_cut(&history).expect("hay historia que resumir");
        let cola = history.len() - cut;
        assert!((10..=12).contains(&cola), "la cola mide {cola} mensajes");
    }

    #[test]
    fn choose_cut_skips_an_existing_summary() {
        // Un resumen y poco más: no queda nada nuevo que resumir.
        let history = vec![
            summary_message(0, "r"),
            Message::user("hola"),
            Message::assistant(vec![ContentPart::Text {
                text: "adiós".into(),
            }]),
        ];
        assert_eq!(choose_cut(&history), None);

        // Un resumen y un mensaje tan grande que ocupa toda la cola: lo único anterior al corte
        // es el propio resumen, así que tampoco hay nada nuevo que resumir.
        let history = vec![summary_message(0, "r"), Message::user("x".repeat(40_000))];
        assert_eq!(choose_cut(&history), None);
    }

    #[test]
    fn choose_cut_never_falls_before_an_existing_summary() {
        // Seis mensajes de 1000 tokens, el resumen que los cubre (posición 6) y cinco más. El
        // presupuesto de cola se agota dentro de los anteriores al resumen, pero el corte no puede
        // caer ahí: el resumen previo quedaría fuera de lo que se resume y se perdería.
        let mut history: Vec<Message> = (0..6).map(|_| Message::user("x".repeat(4000))).collect();
        history.push(summary_message(0, "r"));
        history.extend((0..5).map(|_| Message::user("y".repeat(4000))));
        assert_eq!(choose_cut(&history), Some(7));
    }
}
