use async_trait::async_trait;

use crate::db::Db;
use crate::message::{CommandScope, Destination, MessageContext, Response};
use crate::module::Module;

pub struct WeatherModule {
    latitude: f64,
    longitude: f64,
    units: String,
}

impl WeatherModule {
    pub fn new(latitude: f64, longitude: f64, units: String) -> Self {
        Self {
            latitude,
            longitude,
            units,
        }
    }

    fn temperature_unit(&self) -> &str {
        if self.units == "imperial" {
            "fahrenheit"
        } else {
            "celsius"
        }
    }

    fn temp_symbol(&self) -> &str {
        if self.units == "imperial" {
            "°F"
        } else {
            "°C"
        }
    }

    fn wind_unit(&self) -> &str {
        if self.units == "imperial" {
            "mph"
        } else {
            "kmh"
        }
    }

    fn wind_symbol(&self) -> &str {
        if self.units == "imperial" {
            "mph"
        } else {
            "km/h"
        }
    }
}

fn wmo_code_to_description(code: u64) -> &'static str {
    match code {
        0 => "Clear sky",
        1 => "Mainly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Foggy",
        51 | 53 | 55 => "Drizzle",
        56 | 57 => "Freezing drizzle",
        61 | 63 | 65 => "Rain",
        66 | 67 => "Freezing rain",
        71 | 73 | 75 => "Snowfall",
        77 => "Snow grains",
        80..=82 => "Rain showers",
        85 | 86 => "Snow showers",
        95 => "Thunderstorm",
        96 | 99 => "Thunderstorm w/ hail",
        _ => "Unknown",
    }
}

#[async_trait]
impl Module for WeatherModule {
    fn name(&self) -> &str {
        "weather"
    }

    fn description(&self) -> &str {
        "Weather forecast"
    }

    fn commands(&self) -> &[&str] {
        &["weather"]
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
        // Use sender's position if available, otherwise fall back to configured default
        let (lat, lon, location_note) = match db.get_node_position(ctx.sender_id)? {
            Some((lat, lon)) => (lat, lon, " (your location)"),
            None => (self.latitude, self.longitude, ""),
        };

        let url = format!(
            "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}\
             &current=temperature_2m,relative_humidity_2m,weather_code,wind_speed_10m\
             &temperature_unit={}&wind_speed_unit={}",
            lat,
            lon,
            self.temperature_unit(),
            self.wind_unit(),
        );

        let resp = reqwest::get(&url).await.map_err(|e| {
            log::error!("Weather API request failed: {}", e);
            e
        })?;

        if !resp.status().is_success() {
            let status = resp.status();
            log::error!("Weather API returned HTTP {}", status);
            return Ok(Some(vec![Response {
                text: format!("Weather unavailable (HTTP {})", status.as_u16()),
                destination: Destination::Sender,
                channel: ctx.channel,
                reply_id: None,
            }]));
        }

        let json: serde_json::Value = resp.json().await?;

        let current = match json.get("current") {
            Some(c) if c.is_object() => c,
            _ => {
                log::error!("Weather API response missing 'current' object: {}", json);
                return Ok(Some(vec![Response {
                    text: "Weather unavailable (bad API response)".to_string(),
                    destination: Destination::Sender,
                    channel: ctx.channel,
                    reply_id: None,
                }]));
            }
        };

        let temp = current["temperature_2m"].as_f64().unwrap_or(0.0);
        let humidity = current["relative_humidity_2m"].as_f64().unwrap_or(0.0);
        let weather_code = current["weather_code"].as_u64().unwrap_or(0);
        let wind = current["wind_speed_10m"].as_f64().unwrap_or(0.0);

        let conditions = wmo_code_to_description(weather_code);

        let text = format!(
            "Weather{}: {:.0}{} {}\nHumidity: {:.0}% Wind: {:.0}{}",
            location_note,
            temp,
            self.temp_symbol(),
            conditions,
            humidity,
            wind,
            self.wind_symbol(),
        );

        Ok(Some(vec![Response {
            text,
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
    fn test_wmo_codes() {
        assert_eq!(wmo_code_to_description(0), "Clear sky");
        assert_eq!(wmo_code_to_description(1), "Mainly clear");
        assert_eq!(wmo_code_to_description(2), "Partly cloudy");
        assert_eq!(wmo_code_to_description(3), "Overcast");
        assert_eq!(wmo_code_to_description(45), "Foggy");
        assert_eq!(wmo_code_to_description(48), "Foggy");
        assert_eq!(wmo_code_to_description(61), "Rain");
        assert_eq!(wmo_code_to_description(80), "Rain showers");
        assert_eq!(wmo_code_to_description(81), "Rain showers");
        assert_eq!(wmo_code_to_description(82), "Rain showers");
        assert_eq!(wmo_code_to_description(95), "Thunderstorm");
        assert_eq!(wmo_code_to_description(96), "Thunderstorm w/ hail");
        assert_eq!(wmo_code_to_description(999), "Unknown");
    }

    #[test]
    fn test_metric_units() {
        let module = WeatherModule::new(25.0, 121.0, "metric".to_string());
        assert_eq!(module.temperature_unit(), "celsius");
        assert_eq!(module.temp_symbol(), "°C");
        assert_eq!(module.wind_unit(), "kmh");
        assert_eq!(module.wind_symbol(), "km/h");
    }

    #[test]
    fn test_imperial_units() {
        let module = WeatherModule::new(25.0, 121.0, "imperial".to_string());
        assert_eq!(module.temperature_unit(), "fahrenheit");
        assert_eq!(module.temp_symbol(), "°F");
        assert_eq!(module.wind_unit(), "mph");
        assert_eq!(module.wind_symbol(), "mph");
    }

    #[test]
    fn test_module_metadata() {
        let module = WeatherModule::new(25.0, 121.0, "metric".to_string());
        assert_eq!(module.name(), "weather");
        assert_eq!(module.commands(), &["weather"]);
        assert_eq!(module.scope(), CommandScope::Both);
    }
}
