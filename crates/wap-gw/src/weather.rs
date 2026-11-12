//! Compact ZIP-code weather pages for the carrier portal.
use crate::portal::{escape, page};
use axum::http::HeaderMap;
use gw_transcode::proxy::Proxy;
use serde_json::Value;

async fn json(proxy: &Proxy, url: &str) -> Result<Value, String> {
    let result = proxy
        .get(url, &HeaderMap::new())
        .await
        .map_err(|e| e.to_string())?;
    if result.status != 200 {
        return Err(format!("Weather service returned {}", result.status));
    }
    serde_json::from_slice(&result.bytes).map_err(|e| e.to_string())
}

pub async fn lookup(proxy: &Proxy, zip: &str) -> String {
    let url = format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={zip}&count=10&countryCode=US"
    );
    match json(proxy, &url).await {
        Ok(data) => locations(&data, zip),
        Err(_) => page(
            "Weather",
            "<p>Weather lookup is unavailable. Please try again.</p>",
        ),
    }
}

fn locations(data: &Value, zip: &str) -> String {
    let mut body = String::new();
    if let Some(results) = data["results"].as_array() {
        for entry in results {
            let (Some(name), Some(lat), Some(lon)) = (
                entry["name"].as_str(),
                entry["latitude"].as_f64(),
                entry["longitude"].as_f64(),
            ) else {
                continue;
            };
            let state = entry["admin1"].as_str().unwrap_or("");
            let label = if state.is_empty() {
                name.to_string()
            } else {
                format!("{name}, {state}")
            };
            let query = url::form_urlencoded::Serializer::new(String::new())
                .append_pair("lat", &lat.to_string())
                .append_pair("lon", &lon.to_string())
                .append_pair("name", &label)
                .finish();
            body.push_str(&format!(
                "<p><a href=\"http://mw2.vzwwap.com/weather/forecast?{}\">{}</a></p>",
                escape(&query),
                escape(&label)
            ));
        }
    }
    if body.is_empty() {
        body.push_str("<p>No locations found. Try another ZIP code.</p>");
    }
    body.push_str("<p><a href=\"http://mw2.vzwwap.com/weather\">Change ZIP</a></p><p>Location data: GeoNames / Open-Meteo</p>");
    page(&format!("Weather: {zip}"), &body)
}

pub async fn forecast(proxy: &Proxy, lat: f64, lon: f64, name: &str) -> String {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}&current=temperature_2m,weather_code,wind_speed_10m&daily=temperature_2m_max,temperature_2m_min,precipitation_probability_max&temperature_unit=fahrenheit&wind_speed_unit=mph&timezone=auto&forecast_days=1"
    );
    match json(proxy, &url).await {
        Ok(data) => forecast_page(&data, name),
        Err(_) => page("Weather", "<p>Forecast unavailable. Please try again.</p>"),
    }
}

fn condition(code: i64) -> &'static str {
    match code {
        0 => "Clear",
        1 => "Mostly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Fog",
        51..=57 => "Drizzle",
        61..=67 => "Rain",
        71..=77 => "Snow",
        80..=82 => "Rain showers",
        85 | 86 => "Snow showers",
        95..=99 => "Thunderstorm",
        _ => "Conditions unavailable",
    }
}

fn forecast_page(data: &Value, name: &str) -> String {
    let current = &data["current"];
    let Some(temp) = current["temperature_2m"].as_f64() else {
        return page("Weather", "<p>Forecast unavailable.</p>");
    };
    let code = current["weather_code"].as_i64().unwrap_or(-1);
    let mut body = format!(
        "<p>{}</p><p>{temp:.0} F - {}</p>",
        escape(name),
        condition(code)
    );
    if let (Some(high), Some(low)) = (
        data["daily"]["temperature_2m_max"][0].as_f64(),
        data["daily"]["temperature_2m_min"][0].as_f64(),
    ) {
        body.push_str(&format!("<p>Today: High {high:.0} F / Low {low:.0} F</p>"));
    }
    if let Some(rain) = data["daily"]["precipitation_probability_max"][0].as_f64() {
        body.push_str(&format!("<p>Precipitation chance: {rain:.0}%</p>"));
    }
    if let Some(wind) = current["wind_speed_10m"].as_f64() {
        body.push_str(&format!("<p>Wind: {wind:.0} mph</p>"));
    }
    body.push_str("<p>Weather: Open-Meteo</p><p><a href=\"http://mw2.vzwwap.com/weather\">Change ZIP</a></p><p><a href=\"http://homepage/\">Home</a></p>");
    page("Weather", &body)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn locations_escape_names_and_offer_selection() {
        let data = serde_json::json!({"results":[{"name":"A&B", "admin1":"New York", "latitude":40.0, "longitude":-74.0}]});
        let result = locations(&data, "10001");
        assert!(result.contains("A&amp;B, New York"));
        assert!(result.contains("weather/forecast?lat=40&amp;lon=-74"));
        assert!(locations(&serde_json::json!({}), "00000").contains("No locations"));
    }
    #[test]
    fn forecast_formats_weather_and_handles_missing_data() {
        let data = serde_json::json!({"current":{"temperature_2m":65.0,"weather_code":2,"wind_speed_10m":5.0},"daily":{"temperature_2m_max":[70.0],"temperature_2m_min":[50.0],"precipitation_probability_max":[20]}});
        let result = forecast_page(&data, "A&B");
        assert!(result.contains("65 F - Partly cloudy"));
        assert!(result.contains("High 70 F / Low 50 F"));
        assert!(result.contains("A&amp;B"));
        assert!(forecast_page(&serde_json::json!({}), "").contains("unavailable"));
    }
}
