use async_trait::async_trait;
use chrono::Utc;

use crate::db::Db;
use crate::message::{CommandScope, Destination, MessageContext, Response};
use crate::module::Module;
use crate::util::format_ago;

pub struct NodeInfoModule;

#[async_trait]
impl Module for NodeInfoModule {
    fn name(&self) -> &str {
        "nodes"
    }

    fn description(&self) -> &str {
        "Mesh node listing"
    }

    fn commands(&self) -> &[&str] {
        &["nodes"]
    }

    fn scope(&self) -> CommandScope {
        CommandScope::Both
    }

    async fn handle_command(
        &self,
        _command: &str,
        args: &str,
        ctx: &MessageContext,
        db: &Db,
    ) -> Result<Option<Vec<Response>>, Box<dyn std::error::Error + Send + Sync>> {
        let count: usize = args.parse().unwrap_or(5).min(20);

        let total_nodes = db.node_count()? as usize;
        let nodes = db.get_recent_nodes_with_last_hop(count)?;
        let now = Utc::now().timestamp();

        let mut lines = vec![format!("Nodes seen: {}", total_nodes)];
        for node in &nodes {
            let name = if !node.long_name.is_empty() {
                &node.long_name
            } else if !node.short_name.is_empty() {
                &node.short_name
            } else {
                "unknown"
            };
            let ago = format_ago(now - node.last_seen);
            let hops = node
                .last_hop
                .map(|h| format!(" | hops {}", h))
                .unwrap_or_default();
            lines.push(format!("!{:08x} {} ({}){}", node.node_id, name, ago, hops));
        }

        if total_nodes > nodes.len() {
            lines.push(format!("...and {} more", total_nodes - nodes.len()));
        }

        Ok(Some(vec![Response {
            text: lines.join("\n"),
            destination: Destination::Sender,
            channel: ctx.channel,
            reply_id: None,
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
            packet_id: 0,
        }
    }

    #[tokio::test]
    async fn test_nodes_empty() {
        let module = NodeInfoModule;
        let db = Db::open(Path::new(":memory:")).unwrap();
        let ctx = test_context();

        let result = module.handle_command("nodes", "", &ctx, &db).await.unwrap();
        let responses = result.unwrap();

        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].text, "Nodes seen: 0");
    }

    #[tokio::test]
    async fn test_nodes_with_data() {
        let module = NodeInfoModule;
        let db = Db::open(Path::new(":memory:")).unwrap();
        let ctx = test_context();

        // Add some nodes
        db.upsert_node(0xaabbccdd, "ABCD", "Alice's Node", false)
            .unwrap();
        db.upsert_node(0x11223344, "EFGH", "Bob's Node", false)
            .unwrap();

        let result = module.handle_command("nodes", "", &ctx, &db).await.unwrap();
        let responses = result.unwrap();
        let text = &responses[0].text;

        assert!(text.starts_with("Nodes seen: 2"));
        assert!(text.contains("!aabbccdd"));
        assert!(text.contains("Alice's Node"));
        assert!(text.contains("!11223344"));
        assert!(text.contains("Bob's Node"));
    }

    #[tokio::test]
    async fn test_nodes_with_count_argument() {
        let module = NodeInfoModule;
        let db = Db::open(Path::new(":memory:")).unwrap();
        let ctx = test_context();

        // Add 10 nodes
        for i in 0..10u32 {
            db.upsert_node(i, &format!("N{}", i), &format!("Node {}", i), false)
                .unwrap();
        }

        // Request only 3
        let result = module
            .handle_command("nodes", "3", &ctx, &db)
            .await
            .unwrap();
        let responses = result.unwrap();
        let text = &responses[0].text;

        assert!(text.starts_with("Nodes seen: 10"));
        assert!(text.contains("...and 7 more"));
        // Should only have header + 3 nodes + "...and N more" = 5 lines
        assert_eq!(text.lines().count(), 5);
    }

    #[tokio::test]
    async fn test_nodes_max_count_capped() {
        let module = NodeInfoModule;
        let db = Db::open(Path::new(":memory:")).unwrap();
        let ctx = test_context();

        // Add 25 nodes
        for i in 0..25u32 {
            db.upsert_node(i, &format!("N{}", i), &format!("Node {}", i), false)
                .unwrap();
        }

        // Request 100 (should be capped to 20)
        let result = module
            .handle_command("nodes", "100", &ctx, &db)
            .await
            .unwrap();
        let responses = result.unwrap();
        let text = &responses[0].text;

        assert!(text.contains("...and 5 more"));
    }

    #[tokio::test]
    async fn test_nodes_prefers_long_name() {
        let module = NodeInfoModule;
        let db = Db::open(Path::new(":memory:")).unwrap();
        let ctx = test_context();

        db.upsert_node(0x12345678, "SHORT", "Long Name Here", false)
            .unwrap();

        let result = module.handle_command("nodes", "", &ctx, &db).await.unwrap();
        let text = &result.unwrap()[0].text;

        assert!(text.contains("Long Name Here"));
        assert!(!text.contains("SHORT"));
    }

    #[tokio::test]
    async fn test_nodes_falls_back_to_short_name() {
        let module = NodeInfoModule;
        let db = Db::open(Path::new(":memory:")).unwrap();
        let ctx = test_context();

        db.upsert_node(0x12345678, "SHORT", "", false).unwrap();

        let result = module.handle_command("nodes", "", &ctx, &db).await.unwrap();
        let text = &result.unwrap()[0].text;

        assert!(text.contains("SHORT"));
    }

    #[tokio::test]
    async fn test_nodes_unknown_when_no_name() {
        let module = NodeInfoModule;
        let db = Db::open(Path::new(":memory:")).unwrap();
        let ctx = test_context();

        db.upsert_node(0x12345678, "", "", false).unwrap();

        let result = module.handle_command("nodes", "", &ctx, &db).await.unwrap();
        let text = &result.unwrap()[0].text;

        assert!(text.contains("unknown"));
    }

    #[tokio::test]
    async fn test_nodes_includes_hops_when_available() {
        let module = NodeInfoModule;
        let db = Db::open(Path::new(":memory:")).unwrap();
        let ctx = test_context();

        db.upsert_node(0x12345678, "N1", "Node 1", false).unwrap();
        db.log_packet(
            0x12345678,
            None,
            0,
            "hi",
            "in",
            false,
            Some(-80),
            Some(5.0),
            Some(3),
            Some(7),
            "text",
        )
        .unwrap();

        let result = module.handle_command("nodes", "", &ctx, &db).await.unwrap();
        let text = &result.unwrap()[0].text;

        assert!(text.contains("hops 3"));
    }

    #[test]
    fn test_node_info_module_metadata() {
        let module = NodeInfoModule;
        assert_eq!(module.name(), "nodes");
        assert_eq!(module.commands(), &["nodes"]);
        assert_eq!(module.scope(), CommandScope::Both);
    }
}
