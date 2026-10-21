//! Preguntas al usuario (`ask_user`): tipos, validación de la entrada y formato de las respuestas.

use serde_json::Value;

/// Número máximo de preguntas por llamada.
pub const MAX_QUESTIONS: usize = 4;
/// Número mínimo de opciones por pregunta.
pub const MIN_OPTIONS: usize = 2;
/// Número máximo de opciones por pregunta.
pub const MAX_OPTIONS: usize = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub header: String,
    pub question: String,
    pub options: Vec<QuestionOption>,
    pub multi_select: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub question: String,
    pub selected: Vec<String>,
}

/// Valida la entrada de `ask_user` y la convierte en preguntas.
pub fn parse_questions(input: &Value) -> Result<Vec<Question>, String> {
    let items = input
        .get("questions")
        .and_then(Value::as_array)
        .ok_or_else(|| "questions must be an array".to_string())?;
    if items.is_empty() || items.len() > MAX_QUESTIONS {
        return Err(format!(
            "between 1 and {MAX_QUESTIONS} questions are required"
        ));
    }
    items
        .iter()
        .enumerate()
        .map(|(index, item)| parse_question(index + 1, item))
        .collect()
}

/// Valida una pregunta (`n` es su número, base 1, para los mensajes de error).
fn parse_question(n: usize, item: &Value) -> Result<Question, String> {
    let question = text_field(item, "question");
    if question.is_empty() {
        return Err(format!("question {n}: empty question"));
    }
    let raw: &[Value] = item
        .get("options")
        .and_then(Value::as_array)
        .map(|options| options.as_slice())
        .unwrap_or(&[]);
    if !(MIN_OPTIONS..=MAX_OPTIONS).contains(&raw.len()) {
        return Err(format!(
            "question {n}: between {MIN_OPTIONS} and {MAX_OPTIONS} options are required"
        ));
    }
    let mut options = Vec::with_capacity(raw.len());
    for (index, option) in raw.iter().enumerate() {
        let label = text_field(option, "label");
        if label.is_empty() {
            return Err(format!(
                "question {n}: option {} has an empty label",
                index + 1
            ));
        }
        options.push(QuestionOption {
            label,
            description: text_field(option, "description"),
        });
    }
    Ok(Question {
        header: text_field(item, "header"),
        question,
        options,
        multi_select: item
            .get("multi_select")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// Texto opcional de un campo, sin espacios sobrantes; vacío si falta.
fn text_field(item: &Value, key: &str) -> String {
    item.get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

/// Formatea las respuestas: una línea por pregunta, `"{pregunta}: {a1}, {a2}"`.
pub fn format_answers(answers: &[Answer]) -> String {
    answers
        .iter()
        .map(|answer| {
            let picked = if answer.selected.is_empty() {
                "(no answer)".to_string()
            } else {
                answer.selected.join(", ")
            };
            format!("{}: {picked}", answer.question)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_a_valid_question_with_defaults() {
        let questions = parse_questions(&json!({"questions": [{
            "question": "Which?",
            "options": [{"label": "a"}, {"label": "b", "description": "bee"}]
        }]}))
        .unwrap();
        assert_eq!(
            questions,
            vec![Question {
                header: String::new(),
                question: "Which?".into(),
                options: vec![
                    QuestionOption {
                        label: "a".into(),
                        description: String::new(),
                    },
                    QuestionOption {
                        label: "b".into(),
                        description: "bee".into(),
                    },
                ],
                multi_select: false,
            }]
        );
    }

    #[test]
    fn rejects_non_array_and_out_of_range_question_counts() {
        assert_eq!(
            parse_questions(&json!({"questions": "nope"})).unwrap_err(),
            "questions must be an array"
        );
        assert_eq!(
            parse_questions(&json!({"questions": []})).unwrap_err(),
            "between 1 and 4 questions are required"
        );
        let five: Vec<Value> = (0..5)
            .map(|_| json!({"question": "Q", "options": [{"label": "a"}, {"label": "b"}]}))
            .collect();
        assert_eq!(
            parse_questions(&json!({ "questions": five })).unwrap_err(),
            "between 1 and 4 questions are required"
        );
    }

    #[test]
    fn rejects_one_and_seven_options() {
        let one = json!({"questions": [{"question": "Q", "options": [{"label": "a"}]}]});
        assert_eq!(
            parse_questions(&one).unwrap_err(),
            "question 1: between 2 and 6 options are required"
        );
        let seven: Vec<Value> = (0..7).map(|i| json!({"label": format!("o{i}")})).collect();
        let seven = json!({"questions": [{"question": "Q", "options": seven}]});
        assert_eq!(
            parse_questions(&seven).unwrap_err(),
            "question 1: between 2 and 6 options are required"
        );
    }

    #[test]
    fn rejects_empty_question_and_label() {
        let empty_question = json!({"questions": [
            {"question": "ok", "options": [{"label": "a"}, {"label": "b"}]},
            {"question": "  ", "options": [{"label": "a"}, {"label": "b"}]}
        ]});
        assert_eq!(
            parse_questions(&empty_question).unwrap_err(),
            "question 2: empty question"
        );
        let empty_label = json!({"questions": [{
            "question": "Q",
            "options": [{"label": "a"}, {"label": " "}]
        }]});
        assert_eq!(
            parse_questions(&empty_label).unwrap_err(),
            "question 1: option 2 has an empty label"
        );
    }

    #[test]
    fn format_answers_joins_lines_and_marks_empty() {
        let answers = vec![
            Answer {
                question: "Colour?".into(),
                selected: vec!["red".into(), "blue".into()],
            },
            Answer {
                question: "Size?".into(),
                selected: vec![],
            },
        ];
        assert_eq!(
            format_answers(&answers),
            "Colour?: red, blue\nSize?: (no answer)"
        );
    }
}
