use async_trait::async_trait;
use chrono::Utc;

use crate::db::Db;
use crate::message::{CommandScope, Destination, MeshEvent, MessageContext, Response};
use crate::module::Module;
use crate::util::format_ago;

pub struct MailModule;

#[async_trait]
impl Module for MailModule {
    fn name(&self) -> &str {
        "mail"
    }

    fn description(&self) -> &str {
        "Store-and-forward mail"
    }

    fn commands(&self) -> &[&str] {
        &["mail"]
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
        let (subcmd, rest) = match args.split_once(' ') {
            Some((s, r)) => (s, r.trim()),
            None => (args, ""),
        };

        let text = match subcmd {
            "send" => self.cmd_send(rest, ctx, db)?,
            "read" => self.cmd_read(ctx, db)?,
            "list" => self.cmd_list(ctx, db)?,
            "delete" | "del" => self.cmd_delete(rest, ctx, db)?,
            _ => "Usage: mail send <name> <msg> | mail read | mail list | mail delete <id>".to_string(),
        };

        Ok(Some(vec![Response {
            text,
            destination: Destination::Sender,
            channel: ctx.channel,
        }]))
    }

    async fn handle_event(
        &self,
        event: &MeshEvent,
        db: &Db,
    ) -> Result<Option<Vec<Response>>, Box<dyn std::error::Error + Send + Sync>> {
        match event {
            MeshEvent::NodeDiscovered { node_id, .. } => {
                let count = db.count_unread_mail(*node_id)?;
                if count > 0 {
                    let text = format!(
                        "You have {} unread message{}. Send !mail read to view.",
                        count,
                        if count == 1 { "" } else { "s" }
                    );
                    Ok(Some(vec![Response {
                        text,
                        destination: Destination::Node(*node_id),
                        channel: 0,
                    }]))
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }
}

impl MailModule {
    fn cmd_send(&self, args: &str, ctx: &MessageContext, db: &Db) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let (recipient, body) = match args.split_once(' ') {
            Some((r, b)) if !b.trim().is_empty() => (r.trim(), b.trim()),
            _ => return Ok("Usage: mail send <name> <message>".to_string()),
        };

        let to_node = match db.find_node_by_name(recipient)? {
            Some(id) => id,
            None => return Ok(format!("Unknown node: {}", recipient)),
        };

        if to_node == ctx.sender_id {
            return Ok("Can't send mail to yourself.".to_string());
        }

        let to_name = db.get_node_name(to_node)?;
        db.store_mail(ctx.sender_id, to_node, body)?;

        Ok(format!("Mail sent to {}.", to_name))
    }

    fn cmd_read(&self, ctx: &MessageContext, db: &Db) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let mail = db.get_unread_mail(ctx.sender_id)?;

        if mail.is_empty() {
            return Ok("No unread mail.".to_string());
        }

        let now = Utc::now().timestamp();
        let mut lines = Vec::new();
        for msg in &mail {
            let from_name = db.get_node_name(msg.from_node)?;
            let ago = format_ago(now - msg.timestamp);
            lines.push(format!("[{}] {} ({}): {}", msg.id, from_name, ago, msg.body));
            db.mark_mail_read(msg.id)?;
        }

        Ok(lines.join("\n"))
    }

    fn cmd_list(&self, ctx: &MessageContext, db: &Db) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let count = db.count_unread_mail(ctx.sender_id)?;
        if count == 0 {
            Ok("No unread mail.".to_string())
        } else {
            Ok(format!("{} unread message{}.", count, if count == 1 { "" } else { "s" }))
        }
    }

    fn cmd_delete(&self, args: &str, ctx: &MessageContext, db: &Db) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let id: i64 = match args.trim().parse() {
            Ok(id) => id,
            Err(_) => return Ok("Usage: mail delete <id>".to_string()),
        };

        if db.delete_mail(id, ctx.sender_id)? {
            Ok(format!("Mail #{} deleted.", id))
        } else {
            Ok("Mail not found.".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn test_context(sender_id: u32) -> MessageContext {
        MessageContext {
            sender_id,
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

    fn setup_db() -> Db {
        let db = Db::open(Path::new(":memory:")).unwrap();
        // Add some test nodes
        db.upsert_node(0xAAAAAAAA, "AAAA", "Alice", false).unwrap();
        db.upsert_node(0xBBBBBBBB, "BBBB", "Bob", false).unwrap();
        db.upsert_node(0xCCCCCCCC, "CCCC", "Charlie", false).unwrap();
        db
    }

    // --- send subcommand tests ---

    #[tokio::test]
    async fn test_mail_send_by_name() {
        let module = MailModule;
        let db = setup_db();
        let ctx = test_context(0xAAAAAAAA);

        let result = module.handle_command("mail", "send Bob Hello there!", &ctx, &db).await.unwrap();
        let text = &result.unwrap()[0].text;

        assert_eq!(text, "Mail sent to Bob.");
        assert_eq!(db.count_unread_mail(0xBBBBBBBB).unwrap(), 1);
    }

    #[tokio::test]
    async fn test_mail_send_by_hex_id() {
        let module = MailModule;
        let db = setup_db();
        let ctx = test_context(0xAAAAAAAA);

        let result = module.handle_command("mail", "send !bbbbbbbb Test message", &ctx, &db).await.unwrap();
        let text = &result.unwrap()[0].text;

        assert_eq!(text, "Mail sent to Bob.");
    }

    #[tokio::test]
    async fn test_mail_send_unknown_recipient() {
        let module = MailModule;
        let db = setup_db();
        let ctx = test_context(0xAAAAAAAA);

        let result = module.handle_command("mail", "send Unknown Hello", &ctx, &db).await.unwrap();
        let text = &result.unwrap()[0].text;

        assert_eq!(text, "Unknown node: Unknown");
    }

    #[tokio::test]
    async fn test_mail_send_to_self() {
        let module = MailModule;
        let db = setup_db();
        let ctx = test_context(0xAAAAAAAA);

        let result = module.handle_command("mail", "send Alice Hello", &ctx, &db).await.unwrap();
        let text = &result.unwrap()[0].text;

        assert_eq!(text, "Can't send mail to yourself.");
    }

    #[tokio::test]
    async fn test_mail_send_missing_message() {
        let module = MailModule;
        let db = setup_db();
        let ctx = test_context(0xAAAAAAAA);

        let result = module.handle_command("mail", "send Bob", &ctx, &db).await.unwrap();
        let text = &result.unwrap()[0].text;

        assert!(text.contains("Usage:"));
    }

    #[tokio::test]
    async fn test_mail_send_empty_message() {
        let module = MailModule;
        let db = setup_db();
        let ctx = test_context(0xAAAAAAAA);

        let result = module.handle_command("mail", "send Bob   ", &ctx, &db).await.unwrap();
        let text = &result.unwrap()[0].text;

        assert!(text.contains("Usage:"));
    }

    // --- read subcommand tests ---

    #[tokio::test]
    async fn test_mail_read_no_mail() {
        let module = MailModule;
        let db = setup_db();
        let ctx = test_context(0xAAAAAAAA);

        let result = module.handle_command("mail", "read", &ctx, &db).await.unwrap();
        let text = &result.unwrap()[0].text;

        assert_eq!(text, "No unread mail.");
    }

    #[tokio::test]
    async fn test_mail_read_with_mail() {
        let module = MailModule;
        let db = setup_db();
        let ctx = test_context(0xBBBBBBBB);

        // Send mail to Bob from Alice
        db.store_mail(0xAAAAAAAA, 0xBBBBBBBB, "Hello Bob!").unwrap();

        let result = module.handle_command("mail", "read", &ctx, &db).await.unwrap();
        let text = &result.unwrap()[0].text;

        assert!(text.contains("Alice"));
        assert!(text.contains("Hello Bob!"));
        // After reading, mail should be marked as read
        assert_eq!(db.count_unread_mail(0xBBBBBBBB).unwrap(), 0);
    }

    #[tokio::test]
    async fn test_mail_read_multiple() {
        let module = MailModule;
        let db = setup_db();
        let ctx = test_context(0xCCCCCCCC);

        // Send multiple messages to Charlie
        db.store_mail(0xAAAAAAAA, 0xCCCCCCCC, "Message 1").unwrap();
        db.store_mail(0xBBBBBBBB, 0xCCCCCCCC, "Message 2").unwrap();

        let result = module.handle_command("mail", "read", &ctx, &db).await.unwrap();
        let text = &result.unwrap()[0].text;

        assert!(text.contains("Message 1"));
        assert!(text.contains("Message 2"));
        assert!(text.contains("Alice"));
        assert!(text.contains("Bob"));
    }

    // --- list subcommand tests ---

    #[tokio::test]
    async fn test_mail_list_empty() {
        let module = MailModule;
        let db = setup_db();
        let ctx = test_context(0xAAAAAAAA);

        let result = module.handle_command("mail", "list", &ctx, &db).await.unwrap();
        let text = &result.unwrap()[0].text;

        assert_eq!(text, "No unread mail.");
    }

    #[tokio::test]
    async fn test_mail_list_one() {
        let module = MailModule;
        let db = setup_db();
        let ctx = test_context(0xBBBBBBBB);

        db.store_mail(0xAAAAAAAA, 0xBBBBBBBB, "Test").unwrap();

        let result = module.handle_command("mail", "list", &ctx, &db).await.unwrap();
        let text = &result.unwrap()[0].text;

        assert_eq!(text, "1 unread message.");
    }

    #[tokio::test]
    async fn test_mail_list_multiple() {
        let module = MailModule;
        let db = setup_db();
        let ctx = test_context(0xBBBBBBBB);

        db.store_mail(0xAAAAAAAA, 0xBBBBBBBB, "Test 1").unwrap();
        db.store_mail(0xCCCCCCCC, 0xBBBBBBBB, "Test 2").unwrap();

        let result = module.handle_command("mail", "list", &ctx, &db).await.unwrap();
        let text = &result.unwrap()[0].text;

        assert_eq!(text, "2 unread messages.");
    }

    // --- delete subcommand tests ---

    #[tokio::test]
    async fn test_mail_delete_success() {
        let module = MailModule;
        let db = setup_db();
        let ctx = test_context(0xBBBBBBBB);

        let mail_id = db.store_mail(0xAAAAAAAA, 0xBBBBBBBB, "Test").unwrap();

        let result = module.handle_command("mail", &format!("delete {}", mail_id), &ctx, &db).await.unwrap();
        let text = &result.unwrap()[0].text;

        assert_eq!(text, &format!("Mail #{} deleted.", mail_id));
    }

    #[tokio::test]
    async fn test_mail_delete_not_found() {
        let module = MailModule;
        let db = setup_db();
        let ctx = test_context(0xBBBBBBBB);

        let result = module.handle_command("mail", "delete 99999", &ctx, &db).await.unwrap();
        let text = &result.unwrap()[0].text;

        assert_eq!(text, "Mail not found.");
    }

    #[tokio::test]
    async fn test_mail_delete_wrong_owner() {
        let module = MailModule;
        let db = setup_db();

        // Mail to Bob
        let mail_id = db.store_mail(0xAAAAAAAA, 0xBBBBBBBB, "Test").unwrap();

        // Alice tries to delete Bob's mail
        let ctx = test_context(0xAAAAAAAA);
        let result = module.handle_command("mail", &format!("delete {}", mail_id), &ctx, &db).await.unwrap();
        let text = &result.unwrap()[0].text;

        assert_eq!(text, "Mail not found.");
    }

    #[tokio::test]
    async fn test_mail_delete_invalid_id() {
        let module = MailModule;
        let db = setup_db();
        let ctx = test_context(0xAAAAAAAA);

        let result = module.handle_command("mail", "delete abc", &ctx, &db).await.unwrap();
        let text = &result.unwrap()[0].text;

        assert!(text.contains("Usage:"));
    }

    #[tokio::test]
    async fn test_mail_del_alias() {
        let module = MailModule;
        let db = setup_db();
        let ctx = test_context(0xBBBBBBBB);

        let mail_id = db.store_mail(0xAAAAAAAA, 0xBBBBBBBB, "Test").unwrap();

        let result = module.handle_command("mail", &format!("del {}", mail_id), &ctx, &db).await.unwrap();
        let text = &result.unwrap()[0].text;

        assert!(text.contains("deleted"));
    }

    // --- unknown subcommand ---

    #[tokio::test]
    async fn test_mail_unknown_subcommand() {
        let module = MailModule;
        let db = setup_db();
        let ctx = test_context(0xAAAAAAAA);

        let result = module.handle_command("mail", "unknown", &ctx, &db).await.unwrap();
        let text = &result.unwrap()[0].text;

        assert!(text.contains("Usage:"));
    }

    #[tokio::test]
    async fn test_mail_empty_args() {
        let module = MailModule;
        let db = setup_db();
        let ctx = test_context(0xAAAAAAAA);

        let result = module.handle_command("mail", "", &ctx, &db).await.unwrap();
        let text = &result.unwrap()[0].text;

        assert!(text.contains("Usage:"));
    }

    // --- event handling tests ---

    #[tokio::test]
    async fn test_mail_event_notification() {
        let module = MailModule;
        let db = setup_db();

        // Send mail to Bob
        db.store_mail(0xAAAAAAAA, 0xBBBBBBBB, "Test").unwrap();

        // Bob comes online
        let event = MeshEvent::NodeDiscovered {
            node_id: 0xBBBBBBBB,
            long_name: "Bob".to_string(),
            short_name: "BBBB".to_string(),
            via_mqtt: false,
        };

        let result = module.handle_event(&event, &db).await.unwrap();
        assert!(result.is_some());

        let responses = result.unwrap();
        assert_eq!(responses.len(), 1);
        assert!(responses[0].text.contains("1 unread message"));
        assert!(matches!(responses[0].destination, Destination::Node(0xBBBBBBBB)));
    }

    #[tokio::test]
    async fn test_mail_event_no_notification_when_empty() {
        let module = MailModule;
        let db = setup_db();

        // Bob comes online with no mail
        let event = MeshEvent::NodeDiscovered {
            node_id: 0xBBBBBBBB,
            long_name: "Bob".to_string(),
            short_name: "BBBB".to_string(),
            via_mqtt: false,
        };

        let result = module.handle_event(&event, &db).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_mail_event_plural_messages() {
        let module = MailModule;
        let db = setup_db();

        // Send multiple messages to Bob
        db.store_mail(0xAAAAAAAA, 0xBBBBBBBB, "Test 1").unwrap();
        db.store_mail(0xCCCCCCCC, 0xBBBBBBBB, "Test 2").unwrap();

        let event = MeshEvent::NodeDiscovered {
            node_id: 0xBBBBBBBB,
            long_name: "Bob".to_string(),
            short_name: "BBBB".to_string(),
            via_mqtt: false,
        };

        let result = module.handle_event(&event, &db).await.unwrap();
        let text = &result.unwrap()[0].text;

        assert!(text.contains("2 unread messages"));
    }

    #[test]
    fn test_mail_module_metadata() {
        let module = MailModule;
        assert_eq!(module.name(), "mail");
        assert_eq!(module.commands(), &["mail"]);
        assert_eq!(module.scope(), CommandScope::Both);
    }
}
