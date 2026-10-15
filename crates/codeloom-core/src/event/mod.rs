use crate::message::Message;
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub enum LiveEvent {
    TurnStarted,
    TurnEnded,
    Delta { text: String },
    ReasoningDelta { text: String },
    MessageAdded(Message),
    ToolStarted { call_id: String, name: String },
    ToolFinished { call_id: String, is_error: bool },
    Error { message: String },
}

#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<LiveEvent>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(capacity);
        Self { tx }
    }
    pub fn subscribe(&self) -> broadcast::Receiver<LiveEvent> {
        self.tx.subscribe()
    }
    pub fn publish(&self, event: LiveEvent) {
        let _ = self.tx.send(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn publishes_to_subscriber() {
        let bus = EventBus::new(8);
        let mut rx = bus.subscribe();
        bus.publish(LiveEvent::TurnStarted);
        match rx.recv().await.unwrap() {
            LiveEvent::TurnStarted => {}
            other => panic!("unexpected {other:?}"),
        }
    }

    #[tokio::test]
    async fn publish_without_subscriber_is_ok() {
        let bus = EventBus::new(8);
        bus.publish(LiveEvent::Delta { text: "x".into() });
    }
}
