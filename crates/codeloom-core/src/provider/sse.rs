#[derive(Default)]
pub struct SseBuffer {
    buf: String,
}

impl SseBuffer {
    pub fn push(&mut self, chunk: &str) -> Vec<String> {
        self.buf.push_str(chunk);
        let mut out = Vec::new();
        while let Some(pos) = self.buf.find("\n\n") {
            let event: String = self.buf[..pos].to_string();
            self.buf.drain(..pos + 2);
            for line in event.lines() {
                if let Some(rest) = line.strip_prefix("data:") {
                    let data = rest.trim_start();
                    if !data.is_empty() {
                        out.push(data.to_string());
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_complete_event() {
        let mut b = SseBuffer::default();
        assert_eq!(b.push("data: a\n\n"), vec!["a".to_string()]);
    }

    #[test]
    fn buffers_fragmented_event() {
        let mut b = SseBuffer::default();
        assert!(b.push("data: {\"a\":").is_empty());
        assert_eq!(b.push("1}\n\n"), vec!["{\"a\":1}".to_string()]);
    }

    #[test]
    fn handles_two_events_in_one_push() {
        let mut b = SseBuffer::default();
        assert_eq!(
            b.push("data: x\n\ndata: y\n\n"),
            vec!["x".to_string(), "y".to_string()]
        );
    }
}
