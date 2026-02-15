use async_trait::async_trait;

use crate::db::Db;
use crate::message::{CommandScope, Destination, MessageContext, Response};
use crate::module::Module;

pub struct HelpModule;

#[async_trait]
impl Module for HelpModule {
    fn name(&self) -> &str {
        "help"
    }

    fn description(&self) -> &str {
        "List commands"
    }

    fn commands(&self) -> &[&str] {
        &["help"]
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
        // The help text is injected by the bot when it calls this module,
        // since the module itself doesn't have access to the registry.
        // This is a placeholder — the bot overrides the response.
        Ok(Some(vec![Response {
            text: String::new(),
            destination: Destination::Sender,
            channel: ctx.channel,
            reply_id: None,
        }]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_help_module_metadata() {
        let module = HelpModule;
        assert_eq!(module.name(), "help");
        assert_eq!(module.commands(), &["help"]);
        assert_eq!(module.scope(), CommandScope::Both);
    }
}
