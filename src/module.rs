use async_trait::async_trait;

use crate::db::Db;
use crate::message::{CommandScope, MeshEvent, MessageContext, Response};

#[async_trait]
pub trait Module: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn commands(&self) -> &[&str];
    fn scope(&self) -> CommandScope;

    async fn handle_command(
        &self,
        command: &str,
        args: &str,
        ctx: &MessageContext,
        db: &Db,
    ) -> Result<Option<Vec<Response>>, Box<dyn std::error::Error + Send + Sync>>;

    async fn handle_event(
        &self,
        _event: &MeshEvent,
        _db: &Db,
    ) -> Result<Option<Vec<Response>>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }
}

pub struct ModuleRegistry {
    modules: Vec<Box<dyn Module>>,
}

impl ModuleRegistry {
    pub fn new() -> Self {
        Self {
            modules: Vec::new(),
        }
    }

    pub fn register(&mut self, module: Box<dyn Module>) {
        log::info!("Registered module: {}", module.name());
        self.modules.push(module);
    }

    pub fn find_by_command(&self, command: &str) -> Option<&dyn Module> {
        self.modules
            .iter()
            .find(|m| m.commands().contains(&command))
            .map(|m| m.as_ref())
    }

    pub fn all(&self) -> &[Box<dyn Module>] {
        &self.modules
    }
}
