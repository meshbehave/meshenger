use std::time::Instant;

use async_trait::async_trait;

use crate::db::Db;
use crate::message::{CommandScope, Destination, MessageContext, Response};
use crate::module::Module;
use crate::util::format_duration;

pub struct UptimeModule {
    started: Instant,
}

impl UptimeModule {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
        }
    }
}

#[async_trait]
impl Module for UptimeModule {
    fn name(&self) -> &str {
        "uptime"
    }

    fn description(&self) -> &str {
        "Bot uptime & stats"
    }

    fn commands(&self) -> &[&str] {
        &["uptime"]
    }

    fn scope(&self) -> CommandScope {
        CommandScope::Both
    }

    async fn handle_command(
        &self,
        _command: &str,
        _args: &str,
        ctx: &MessageContext,
        db: &Db,
    ) -> Result<Option<Vec<Response>>, Box<dyn std::error::Error + Send + Sync>> {
        let uptime = format_duration(self.started.elapsed().as_secs());
        let msgs_in = db.message_count("in").unwrap_or(0);
        let msgs_out = db.message_count("out").unwrap_or(0);
        let nodes = db.node_count().unwrap_or(0);

        let text = format!(
            "Uptime: {}\nMessages: {} in / {} out\nNodes seen: {}",
            uptime, msgs_in, msgs_out, nodes
        );

        Ok(Some(vec![Response {
            text,
            destination: Destination::Sender,
            channel: ctx.channel,
        }]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn test_context() -> MessageContext {
        MessageContext {
            sender_id: 0x12345678,
            sender_name: "TestNode".to_string(),
            channel: 0,
            is_dm: true,
            rssi: -70,
            snr: 5.0,
            hop_count: 1,
            hop_limit: 3,
            via_mqtt: false,
        }
    }

    #[test]
    fn test_uptime_module_metadata() {
        let module = UptimeModule::new();
        assert_eq!(module.name(), "uptime");
        assert_eq!(module.commands(), &["uptime"]);
        assert_eq!(module.scope(), CommandScope::Both);
    }

    #[tokio::test]
    async fn test_uptime_response_format() {
        let module = UptimeModule::new();
        let db = Db::open(Path::new(":memory:")).unwrap();
        let ctx = test_context();

        let result = module.handle_command("uptime", "", &ctx, &db).await.unwrap();
        assert!(result.is_some());

        let responses = result.unwrap();
        assert_eq!(responses.len(), 1);

        let text = &responses[0].text;
        assert!(text.contains("Uptime:"));
        assert!(text.contains("Messages:"));
        assert!(text.contains("Nodes seen:"));
    }

    #[tokio::test]
    async fn test_uptime_counts_messages() {
        let module = UptimeModule::new();
        let db = Db::open(Path::new(":memory:")).unwrap();
        let ctx = test_context();

        // Log some messages
        db.log_message(0x12345678, None, 0, "test", "in").unwrap();
        db.log_message(0x12345678, None, 0, "test", "in").unwrap();
        db.log_message(0x12345678, Some(0xaaaaaaaa), 0, "reply", "out").unwrap();

        let result = module.handle_command("uptime", "", &ctx, &db).await.unwrap();
        let text = &result.unwrap()[0].text;

        assert!(text.contains("2 in"));
        assert!(text.contains("1 out"));
    }

    #[tokio::test]
    async fn test_uptime_counts_nodes() {
        let module = UptimeModule::new();
        let db = Db::open(Path::new(":memory:")).unwrap();
        let ctx = test_context();

        // Add some nodes
        db.upsert_node(0xAAAAAAAA, "A", "Alice").unwrap();
        db.upsert_node(0xBBBBBBBB, "B", "Bob").unwrap();
        db.upsert_node(0xCCCCCCCC, "C", "Charlie").unwrap();

        let result = module.handle_command("uptime", "", &ctx, &db).await.unwrap();
        let text = &result.unwrap()[0].text;

        assert!(text.contains("Nodes seen: 3"));
    }

    #[tokio::test]
    async fn test_uptime_preserves_channel() {
        let module = UptimeModule::new();
        let db = Db::open(Path::new(":memory:")).unwrap();
        let mut ctx = test_context();
        ctx.channel = 5;

        let result = module.handle_command("uptime", "", &ctx, &db).await.unwrap();
        let responses = result.unwrap();

        assert_eq!(responses[0].channel, 5);
    }
}
