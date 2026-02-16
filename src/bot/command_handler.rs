use crate::message::{Destination, MessageContext, Response};

use super::*;

impl Bot {
    pub(super) async fn dispatch_command_from_text(
        &self,
        my_node_id: u32,
        ctx: &MessageContext,
        trimmed_text: &str,
        is_dm: bool,
    ) {
        let (command, args) = match self.parse_command(trimmed_text) {
            Some(parts) => parts,
            None => return,
        };

        // Rate limit check
        if !self.rate_limiter.check(ctx.sender_id) {
            log::warn!("Rate limited: {} ({})", ctx.sender_name, ctx.sender_id);
            return;
        }

        // Special handling for help: generate text from registry
        if command == "help" {
            let help_text = self.generate_help_text();
            let responses = vec![Response {
                text: help_text,
                destination: Destination::Sender,
                channel: ctx.channel,
                reply_id: Some(ctx.packet_id),
            }];
            self.queue_responses(ctx, &responses, my_node_id);
            return;
        }

        let module = match self.registry.find_by_command(command) {
            Some(m) => m,
            None => return,
        };

        if !module.scope().allows(is_dm) {
            return;
        }

        match module.handle_command(command, args, ctx, &self.db).await {
            Ok(Some(mut responses)) => {
                // Tag the first response as a reply to the incoming message
                if let Some(first) = responses.first_mut() {
                    if first.reply_id.is_none() {
                        first.reply_id = Some(ctx.packet_id);
                    }
                }
                self.queue_responses(ctx, &responses, my_node_id);
            }
            Ok(None) => {}
            Err(e) => {
                log::error!("Module {} error: {}", module.name(), e);
            }
        }
    }

    fn parse_command<'a>(&self, trimmed_text: &'a str) -> Option<(&'a str, &'a str)> {
        let prefix = &self.config.bot.command_prefix;
        let (raw_command, args) = match trimmed_text.split_once(' ') {
            Some((cmd, rest)) => (cmd, rest.trim()),
            None => (trimmed_text, ""),
        };

        raw_command
            .strip_prefix(prefix.as_str())
            .map(|cmd| (cmd, args))
    }

    pub(super) fn generate_help_text(&self) -> String {
        let prefix = &self.config.bot.command_prefix;
        let mut lines = Vec::new();
        for module in self.registry.all() {
            let cmds = module.commands();
            if !cmds.is_empty() {
                let cmd_str = cmds
                    .iter()
                    .map(|c| format!("{}{}", prefix, c))
                    .collect::<Vec<_>>()
                    .join(", ");
                lines.push(format!("{} - {}", cmd_str, module.description()));
            }
        }
        if lines.is_empty() {
            "No commands available.".to_string()
        } else {
            lines.join("\n")
        }
    }
}
