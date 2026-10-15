#[derive(Default)]
pub struct SseBuffer {
    buf: String,
}

impl SseBuffer {
    pub fn push_events(&mut self, chunk: &str) -> Vec<(String, String)> {
        self.buf.push_str(chunk);
        let mut out = Vec::new();
        while let Some(pos) = self.buf.find("\n\n") {
            let event: String = self.buf[..pos].to_string();
            self.buf.drain(..pos + 2);
            let mut name = String::from("message");
            let mut data = String::new();
            for line in event.lines() {
                if let Some(r) = line.strip_prefix("event:") {
                    name = r.trim().to_string();
                }
                if let Some(r) = line.strip_prefix("data:") {
                    data = r.trim_start().to_string();
                }
            }
            if !data.is_empty() {
                out.push((name, data));
            }
        }
        out
    }

    pub fn push(&mut self, chunk: &str) -> Vec<String> {
        self.push_events(chunk)
            .into_iter()
            .map(|(_, d)| d)
            .collect()
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

    #[test]
    fn push_events_exposes_event_name() {
        let mut b = SseBuffer::default();
        assert_eq!(
            b.push_events("event: content_block_delta\ndata: {\"i\":0}\n\n"),
            vec![("content_block_delta".to_string(), "{\"i\":0}".to_string())]
        );
    }

    #[test]
    fn push_events_defaults_to_message() {
        let mut b = SseBuffer::default();
        assert_eq!(
            b.push_events("data: z\n\n"),
            vec![("message".to_string(), "z".to_string())]
        );
    }
}
