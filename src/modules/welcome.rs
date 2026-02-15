use std::collections::HashSet;

use async_trait::async_trait;

use crate::db::Db;
use crate::message::{CommandScope, Destination, MeshEvent, MessageContext, Response};
use crate::module::Module;
use crate::util::parse_node_id;

pub struct WelcomeModule {
    message: String,
    welcome_back_message: String,
    absence_threshold_hours: u64,
    whitelist: Option<HashSet<u32>>,
}

impl WelcomeModule {
    pub fn new(
        message: String,
        welcome_back_message: String,
        absence_threshold_hours: u64,
        whitelist: Vec<String>,
    ) -> Self {
        let whitelist = if whitelist.is_empty() {
            None
        } else {
            let ids: HashSet<u32> = whitelist.iter().filter_map(|s| parse_node_id(s)).collect();
            log::info!("Welcome whitelist: {} node(s)", ids.len());
            Some(ids)
        };
        Self {
            message,
            welcome_back_message,
            absence_threshold_hours,
            whitelist,
        }
    }

    fn is_allowed(&self, node_id: u32) -> bool {
        match &self.whitelist {
            None => true,
            Some(ids) => ids.contains(&node_id),
        }
    }

    fn format_message(&self, template: &str, name: &str) -> String {
        template.replace("{name}", name)
    }
}

#[async_trait]
impl Module for WelcomeModule {
    fn name(&self) -> &str {
        "welcome"
    }

    fn description(&self) -> &str {
        "New node greeting"
    }

    fn commands(&self) -> &[&str] {
        &[]
    }

    fn scope(&self) -> CommandScope {
        CommandScope::DM
    }

    async fn handle_command(
        &self,
        _command: &str,
        _args: &str,
        _ctx: &MessageContext,
        _db: &Db,
    ) -> Result<Option<Vec<Response>>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }

    async fn handle_event(
        &self,
        event: &MeshEvent,
        db: &Db,
    ) -> Result<Option<Vec<Response>>, Box<dyn std::error::Error + Send + Sync>> {
        match event {
            MeshEvent::NodeDiscovered {
                node_id,
                long_name,
                short_name,
                ..
            } => {
                if !self.is_allowed(*node_id) {
                    return Ok(None);
                }

                let display_name = if !long_name.is_empty() {
                    long_name.as_str()
                } else if !short_name.is_empty() {
                    short_name.as_str()
                } else {
                    "friend"
                };

                let is_new = db.is_node_new(*node_id)?;
                let is_absent = if !is_new {
                    db.is_node_absent(*node_id, self.absence_threshold_hours)?
                } else {
                    false
                };

                // Update node in DB before deciding on message
                db.upsert_node(*node_id, short_name, long_name, false)?;

                let text = if is_new {
                    log::info!("New node discovered: {} ({})", display_name, node_id);
                    Some(self.format_message(&self.message, display_name))
                } else if is_absent {
                    log::info!("Returning node: {} ({})", display_name, node_id);
                    Some(self.format_message(&self.welcome_back_message, display_name))
                } else {
                    None
                };

                if let Some(text) = text {
                    db.mark_welcomed(*node_id)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn create_module(whitelist: Vec<&str>) -> WelcomeModule {
        WelcomeModule::new(
            "Welcome, {name}!".to_string(),
            "Welcome back, {name}!".to_string(),
            48,
            whitelist.into_iter().map(|s| s.to_string()).collect(),
        )
    }

    #[test]
    fn test_welcome_module_metadata() {
        let module = create_module(vec![]);
        assert_eq!(module.name(), "welcome");
        assert_eq!(module.commands().len(), 0);
        assert_eq!(module.scope(), CommandScope::DM);
    }

    #[test]
    fn test_is_allowed_no_whitelist() {
        let module = create_module(vec![]);
        assert!(module.is_allowed(0x12345678));
        assert!(module.is_allowed(0xAAAAAAAA));
    }

    #[test]
    fn test_is_allowed_with_whitelist() {
        let module = create_module(vec!["!12345678", "!aabbccdd"]);
        assert!(module.is_allowed(0x12345678));
        assert!(module.is_allowed(0xaabbccdd));
        assert!(!module.is_allowed(0x99999999));
    }

    #[test]
    fn test_format_message() {
        let module = create_module(vec![]);
        assert_eq!(module.format_message("Hello, {name}!", "Alice"), "Hello, Alice!");
        assert_eq!(module.format_message("Hi {name}, welcome {name}!", "Bob"), "Hi Bob, welcome Bob!");
    }

    #[tokio::test]
    async fn test_welcome_new_node() {
        let module = create_module(vec![]);
        let db = Db::open(Path::new(":memory:")).unwrap();

        let event = MeshEvent::NodeDiscovered {
            node_id: 0x12345678,
            long_name: "Alice".to_string(),
            short_name: "AAAA".to_string(),
            via_mqtt: false,
        };

        let result = module.handle_event(&event, &db).await.unwrap();
        assert!(result.is_some());

        let responses = result.unwrap();
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].text, "Welcome, Alice!");
        assert!(matches!(responses[0].destination, Destination::Node(0x12345678)));
    }

    #[tokio::test]
    async fn test_welcome_existing_node_no_message() {
        let module = create_module(vec![]);
        let db = Db::open(Path::new(":memory:")).unwrap();

        // Node already exists and was seen recently
        db.upsert_node(0x12345678, "AAAA", "Alice", false).unwrap();

        let event = MeshEvent::NodeDiscovered {
            node_id: 0x12345678,
            long_name: "Alice".to_string(),
            short_name: "AAAA".to_string(),
            via_mqtt: false,
        };

        let result = module.handle_event(&event, &db).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_welcome_whitelist_blocks() {
        let module = create_module(vec!["!aabbccdd"]);
        let db = Db::open(Path::new(":memory:")).unwrap();

        let event = MeshEvent::NodeDiscovered {
            node_id: 0x12345678, // Not in whitelist
            long_name: "Alice".to_string(),
            short_name: "AAAA".to_string(),
            via_mqtt: false,
        };

        let result = module.handle_event(&event, &db).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_welcome_whitelist_allows() {
        let module = create_module(vec!["!12345678"]);
        let db = Db::open(Path::new(":memory:")).unwrap();

        let event = MeshEvent::NodeDiscovered {
            node_id: 0x12345678, // In whitelist
            long_name: "Alice".to_string(),
            short_name: "AAAA".to_string(),
            via_mqtt: false,
        };

        let result = module.handle_event(&event, &db).await.unwrap();
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_welcome_uses_short_name_fallback() {
        let module = create_module(vec![]);
        let db = Db::open(Path::new(":memory:")).unwrap();

        let event = MeshEvent::NodeDiscovered {
            node_id: 0x12345678,
            long_name: "".to_string(),
            short_name: "AAAA".to_string(),
            via_mqtt: false,
        };

        let result = module.handle_event(&event, &db).await.unwrap();
        let text = &result.unwrap()[0].text;
        assert_eq!(text, "Welcome, AAAA!");
    }

    #[tokio::test]
    async fn test_welcome_uses_friend_fallback() {
        let module = create_module(vec![]);
        let db = Db::open(Path::new(":memory:")).unwrap();

        let event = MeshEvent::NodeDiscovered {
            node_id: 0x12345678,
            long_name: "".to_string(),
            short_name: "".to_string(),
            via_mqtt: false,
        };

        let result = module.handle_event(&event, &db).await.unwrap();
        let text = &result.unwrap()[0].text;
        assert_eq!(text, "Welcome, friend!");
    }

    #[tokio::test]
    async fn test_welcome_ignores_position_update() {
        let module = create_module(vec![]);
        let db = Db::open(Path::new(":memory:")).unwrap();

        let event = MeshEvent::PositionUpdate {
            node_id: 0x12345678,
            lat: 25.0,
            lon: 121.0,
            altitude: 100,
        };

        let result = module.handle_event(&event, &db).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_welcome_marks_welcomed() {
        let module = create_module(vec![]);
        let db = Db::open(Path::new(":memory:")).unwrap();

        let event = MeshEvent::NodeDiscovered {
            node_id: 0x12345678,
            long_name: "Alice".to_string(),
            short_name: "AAAA".to_string(),
            via_mqtt: false,
        };

        // First event sends welcome
        let result = module.handle_event(&event, &db).await.unwrap();
        assert!(result.is_some());

        // Second event (node already seen) sends nothing
        let result = module.handle_event(&event, &db).await.unwrap();
        assert!(result.is_none());
    }
}
