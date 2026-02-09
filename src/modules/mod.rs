mod help;
mod mail;
mod node_info;
mod ping;
mod uptime;
mod weather;
mod welcome;

use crate::config::Config;
use crate::module::ModuleRegistry;

pub fn build_registry(config: &Config) -> ModuleRegistry {
    let mut registry = ModuleRegistry::new();

    if config.is_module_enabled("ping") {
        registry.register(Box::new(ping::PingModule));
    }
    if config.is_module_enabled("nodes") {
        registry.register(Box::new(node_info::NodeInfoModule));
    }
    if config.is_module_enabled("weather") {
        registry.register(Box::new(weather::WeatherModule::new(
            config.weather.latitude,
            config.weather.longitude,
            config.weather.units.clone(),
        )));
    }
    if config.is_module_enabled("welcome") {
        registry.register(Box::new(welcome::WelcomeModule::new(
            config.welcome.message.clone(),
            config.welcome.welcome_back_message.clone(),
            config.welcome.absence_threshold_hours,
            config.welcome.whitelist.clone(),
        )));
    }
    if config.is_module_enabled("mail") {
        registry.register(Box::new(mail::MailModule));
    }
    if config.is_module_enabled("uptime") {
        registry.register(Box::new(uptime::UptimeModule::new()));
    }
    if config.is_module_enabled("help") {
        registry.register(Box::new(help::HelpModule));
    }

    registry
}
