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
        80 | 81 | 82 => "Rain showers",
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

        let resp = reqwest::get(&url).await?;
        let json: serde_json::Value = resp.json().await?;

        let current = &json["current"];
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
        }]))
    }
}
