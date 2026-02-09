use async_trait::async_trait;

use crate::db::Db;
use crate::message::{CommandScope, Destination, MessageContext, Response};
use crate::module::Module;

pub struct PingModule;

#[async_trait]
impl Module for PingModule {
    fn name(&self) -> &str {
        "ping"
    }

    fn description(&self) -> &str {
        "Signal report"
    }

    fn commands(&self) -> &[&str] {
        &["ping"]
    }

    fn scope(&self) -> CommandScope {
        CommandScope::Both
    }

    async fn handle_command(
        &self,
        _command: &str,
        _args: &str,
        ctx: &MessageContext,
        _db: &Db,
    ) -> Result<Option<Vec<Response>>, Box<dyn std::error::Error + Send + Sync>> {
        let mqtt_tag = if ctx.via_mqtt { " (via MQTT)" } else { "" };
        let text = format!(
            "Pong! RSSI: {} SNR: {:.1} Hops: {}/{}{}",
            ctx.rssi, ctx.snr, ctx.hop_count, ctx.hop_limit, mqtt_tag
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

    fn test_context(rssi: i32, snr: f32, hop_count: u32, hop_limit: u32, via_mqtt: bool) -> MessageContext {
        MessageContext {
            sender_id: 0x12345678,
            sender_name: "TestNode".to_string(),
            channel: 0,
            is_dm: true,
            rssi,
            snr,
            hop_count,
            hop_limit,
            via_mqtt,
        }
    }

    #[tokio::test]
    async fn test_ping_basic() {
        let module = PingModule;
        let db = Db::open(Path::new(":memory:")).unwrap();
        let ctx = test_context(-70, 5.5, 1, 3, false);

        let result = module.handle_command("ping", "", &ctx, &db).await.unwrap();
        assert!(result.is_some());

        let responses = result.unwrap();
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].text, "Pong! RSSI: -70 SNR: 5.5 Hops: 1/3");
        assert!(matches!(responses[0].destination, Destination::Sender));
    }

    #[tokio::test]
    async fn test_ping_via_mqtt() {
        let module = PingModule;
        let db = Db::open(Path::new(":memory:")).unwrap();
        let ctx = test_context(-80, 3.0, 2, 5, true);

        let result = module.handle_command("ping", "", &ctx, &db).await.unwrap();
        let responses = result.unwrap();

        assert!(responses[0].text.contains("(via MQTT)"));
        assert_eq!(responses[0].text, "Pong! RSSI: -80 SNR: 3.0 Hops: 2/5 (via MQTT)");
    }

    #[tokio::test]
    async fn test_ping_preserves_channel() {
        let module = PingModule;
        let db = Db::open(Path::new(":memory:")).unwrap();
        let mut ctx = test_context(-70, 5.0, 0, 3, false);
        ctx.channel = 5;

        let result = module.handle_command("ping", "", &ctx, &db).await.unwrap();
        let responses = result.unwrap();

        assert_eq!(responses[0].channel, 5);
    }

    #[test]
    fn test_ping_module_metadata() {
        let module = PingModule;
        assert_eq!(module.name(), "ping");
        assert_eq!(module.commands(), &["ping"]);
        assert_eq!(module.scope(), CommandScope::Both);
    }
}
