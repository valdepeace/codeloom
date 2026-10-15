use crate::error::Result;
use crate::message::{FinishReason, Message, Usage};
use async_trait::async_trait;
use std::pin::Pin;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct ProviderRequest {
    pub model: String,
    pub system: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSpec>,
    pub temperature: f32,
    pub max_tokens: u32,
}

#[derive(Debug, Clone)]
pub enum StreamChunk {
    Started,
    ReasoningDelta(String),
    TextDelta(String),
    ToolInputDelta {
        id: String,
        json: String,
    },
    ToolCallComplete {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    Usage(Usage),
    Done(FinishReason),
}

#[derive(Debug, Clone, Copy)]
pub struct Capabilities {
    pub tool_use: bool,
    pub reasoning: bool,
    pub streaming: bool,
}

pub type ProviderStream = Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>>;

#[async_trait]
pub trait Provider: Send + Sync {
    fn capabilities(&self) -> Capabilities;
    async fn stream(
        &self,
        req: ProviderRequest,
        cancel: CancellationToken,
    ) -> Result<ProviderStream>;
}

#[cfg(test)]
pub(crate) mod testutil {
    use super::*;
    use futures::stream;

    pub struct ScriptedProvider {
        pub script: Vec<StreamChunk>,
    }

    #[async_trait::async_trait]
    impl Provider for ScriptedProvider {
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                tool_use: true,
                reasoning: false,
                streaming: true,
            }
        }
        async fn stream(
            &self,
            _req: ProviderRequest,
            _cancel: tokio_util::sync::CancellationToken,
        ) -> Result<ProviderStream> {
            let items = self.script.clone();
            Ok(Box::pin(stream::iter(items)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::ScriptedProvider;
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn mock_provider_yields_scripted_chunks() {
        let p = ScriptedProvider {
            script: vec![
                StreamChunk::Started,
                StreamChunk::TextDelta("hi".into()),
                StreamChunk::Done(FinishReason::Stop),
            ],
        };
        let req = ProviderRequest {
            model: "m".into(),
            system: String::new(),
            messages: vec![],
            tools: vec![],
            temperature: 0.0,
            max_tokens: 16,
        };
        let mut s = p
            .stream(req, tokio_util::sync::CancellationToken::new())
            .await
            .unwrap();
        let mut got = vec![];
        while let Some(c) = s.next().await {
            got.push(c);
        }
        assert_eq!(got.len(), 3);
    }
}
