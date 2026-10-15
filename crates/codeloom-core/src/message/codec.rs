use super::Message;

/// Serializa un mensaje a JSON compacto para persistencia.
pub fn to_json(m: &Message) -> crate::error::Result<String> {
    Ok(serde_json::to_string(m)?)
}

/// Deserializa un mensaje persistido.
pub fn from_json(s: &str) -> crate::error::Result<Message> {
    Ok(serde_json::from_str(s)?)
}
