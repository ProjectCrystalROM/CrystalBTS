//! Small carrier portal for HTTP-proxy handsets.
use crate::portal_store::Store;
use axum::http::Uri;
use std::collections::HashMap;
use std::path::Path;

pub struct Portal {
    pub store: Store,
}

pub enum Action {
    Page(String),
    Redirect,
    Unavailable,
    Fetch(String),
    WeatherLookup(String),
    WeatherForecast {
        latitude: f64,
        longitude: f64,
        name: String,
    },
    Pass,
}

pub(crate) fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub(crate) fn page(title: &str, body: &str) -> String {
    format!(
        "<html><head><title>{}</title></head><body><h1>{}</h1>{}</body></html>",
        escape(title),
        escape(title),
        body
    )
}

impl Portal {
    pub fn new(path: impl AsRef<Path>) -> std::io::Result<Self> {
        Ok(Self {
            store: Store::new(path)?,
        })
    }
    pub fn handle(&self, uri: &Uri, session: &str) -> Action {
        let Some(host) = uri.host() else {
            return Action::Pass;
        };
        let params: HashMap<String, String> =
            url::form_urlencoded::parse(uri.query().unwrap_or("").as_bytes())
                .into_owned()
                .collect();
        if host.eq_ignore_ascii_case("homepage") {
            return Action::Redirect;
        }
        if !host.eq_ignore_ascii_case("mw2.vzwwap.com") {
            return Action::Pass;
        }
        if uri.path() == "/" {
            let Ok(settings) = self.store.read(session) else {
                return Action::Unavailable;
            };
            let mut body = String::from(
                "<p><a href=\"http://mw2.vzwwap.com/goto\">Go to URL</a></p><p><a href=\"http://mw2.vzwwap.com/search\">Search</a></p><p><a href=\"http://mw2.vzwwap.com/favorites\">Bookmarks</a></p>",
            );
            if let Some((lat, lon, name)) = settings.weather {
                let query = url::form_urlencoded::Serializer::new(String::new())
                    .append_pair("lat", &lat.to_string())
                    .append_pair("lon", &lon.to_string())
                    .append_pair("name", &name)
                    .finish();
                body.push_str(&format!(
                    "<p><a href=\"http://mw2.vzwwap.com/weather/forecast?{}\">Weather: {}</a></p>",
                    escape(&query),
                    escape(&name)
                ));
            } else {
                body.push_str("<p><a href=\"http://mw2.vzwwap.com/weather\">Weather</a></p>");
            }
            body.push_str("<p><a href=\"http://mw2.vzwwap.com/open?url=https%3A%2F%2Fen.wikipedia.org%2Fwiki%2FMain_Page\">Wikipedia</a></p>");
            return Action::Page(page("Kozume Wireless", &body));
        }
        if let Some(encoded) = uri.path().strip_prefix("/submit/") {
            let parameter = format!("target={encoded}");
            let Some((_, target)) = url::form_urlencoded::parse(parameter.as_bytes()).next() else {
                return Action::Page(page("Bad request", "<p>Missing form address.</p>"));
            };
            let Ok(mut target) = url::Url::parse(&target) else {
                return Action::Page(page("Bad request", "<p>Invalid form address.</p>"));
            };
            if !matches!(target.scheme(), "http" | "https") {
                return Action::Page(page("Bad request", "<p>Invalid form address.</p>"));
            }
            if let Some(query) = uri.query() {
                let mut pairs = target.query_pairs_mut();
                for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
                    pairs.append_pair(&key, &value);
                }
            }
            return Action::Fetch(target.into());
        }
        match uri.path() {
            "/weather" => Action::Page(page(
                "Weather",
                "<form action=\"http://mw2.vzwwap.com/weather/lookup\" method=\"get\"><p>US ZIP code:</p><input name=\"zip\" type=\"text\"/><input type=\"submit\" value=\"Find\"/></form>",
            )),
            "/weather/lookup" => {
                let zip = params.get("zip").map(String::as_str).unwrap_or("").trim();
                if zip.len() != 5 || !zip.bytes().all(|b| b.is_ascii_digit()) {
                    return Action::Page(page(
                        "Weather",
                        "<p>Enter a five-digit US ZIP code.</p><p><a href=\"http://mw2.vzwwap.com/weather\">Try again</a></p>",
                    ));
                }
                Action::WeatherLookup(zip.to_string())
            }
            "/weather/forecast" => {
                let lat = params.get("lat").and_then(|s| s.parse::<f64>().ok());
                let lon = params.get("lon").and_then(|s| s.parse::<f64>().ok());
                match (lat, lon) {
                    (Some(latitude), Some(longitude))
                        if (-90.0..=90.0).contains(&latitude)
                            && (-180.0..=180.0).contains(&longitude) =>
                    {
                        let name = params.get("name").cloned().unwrap_or_default();
                        if self
                            .store
                            .update(session, |settings| {
                                settings.weather = Some((latitude, longitude, name.clone()))
                            })
                            .is_err()
                        {
                            return Action::Unavailable;
                        }
                        Action::WeatherForecast {
                            latitude,
                            longitude,
                            name,
                        }
                    }
                    _ => Action::Page(page("Weather", "<p>Invalid location.</p>")),
                }
            }
            "/goto" => Action::Page(page(
                "Go to URL",
                "<form action=\"http://mw2.vzwwap.com/open\" method=\"get\"><input name=\"url\" type=\"text\"/><input type=\"submit\" value=\"Go\"/></form>",
            )),
            "/search" => Action::Page(page(
                "Search",
                "<form action=\"http://mw2.vzwwap.com/find\" method=\"get\"><input name=\"q\" type=\"text\"/><input type=\"submit\" value=\"Search\"/></form>",
            )),
            "/open" => {
                let value = params.get("url").map(String::as_str).unwrap_or("").trim();
                if value.is_empty() {
                    return Action::Page(page("Go to URL", "<p>Enter an address.</p>"));
                }
                let target = if value.contains("://") {
                    value.to_string()
                } else {
                    format!("http://{value}")
                };
                Action::Fetch(target)
            }
            "/find" => {
                let query = params.get("q").map(String::as_str).unwrap_or("");
                let encoded: String = url::form_urlencoded::Serializer::new(String::new())
                    .append_pair("q", query)
                    .finish();
                Action::Fetch(format!("https://lite.duckduckgo.com/lite/?{encoded}"))
            }
            "/addfavorite/" => {
                let name = params.get("favname").cloned().unwrap_or_default();
                let target = params.get("favurl").cloned().unwrap_or_default();
                if let Ok(url) = url::Url::parse(&target) {
                    if matches!(url.scheme(), "http" | "https") {
                        if self
                            .store
                            .update(session, |settings| {
                                if !settings.favorites.iter().any(|(_, u)| u == &target) {
                                    settings.favorites.push((name, target));
                                }
                            })
                            .is_err()
                        {
                            return Action::Unavailable;
                        }
                        return Action::Page(page(
                            "Bookmark added",
                            "<p>Saved.</p><p><a href=\"http://mw2.vzwwap.com/favorites\">Bookmarks</a></p>",
                        ));
                    }
                }
                Action::Page(page("Bookmark", "<p>Invalid address.</p>"))
            }
            "/favorites" => {
                let Ok(settings) = self.store.read(session) else {
                    return Action::Unavailable;
                };
                let mut body = String::new();
                {
                    for (name, target) in &settings.favorites {
                        body.push_str(&format!(
                            "<p><a href=\"{}\">{}</a></p>",
                            escape(target),
                            escape(name)
                        ));
                    }
                }
                if body.is_empty() {
                    body.push_str("<p>No bookmarks yet.</p>");
                }
                Action::Page(page("Bookmarks", &body))
            }
            _ => Action::Page(page("Not found", "<p>That portal page is unavailable.</p>")),
        }
    }
}

/// Turn DuckDuckGo Lite results into a compact list, omitting its controls.
pub fn search_results(html: &str) -> Option<gw_transcode::doc::Deck> {
    use gw_transcode::doc::{Block, Deck, Inline};
    let doc = scraper::Html::parse_document(html);
    let selector = scraper::Selector::parse("a.result-link").expect("valid selector");
    let base = url::Url::parse("https://lite.duckduckgo.com/").expect("valid URL");
    let mut deck = Deck {
        title: Some("Search results".to_string()),
        ..Deck::default()
    };
    for element in doc.select(&selector).take(20) {
        let Some(href) = element.value().attr("href") else {
            continue;
        };
        let Ok(mut dest) = base.join(href) else {
            continue;
        };
        if dest.host_str() == Some("duckduckgo.com") && dest.path() == "/l/" {
            if let Some(target) = dest
                .query_pairs()
                .find(|(key, _)| key == "uddg")
                .map(|(_, value)| value.into_owned())
            {
                let Ok(url) = url::Url::parse(&target) else {
                    continue;
                };
                dest = url;
            }
        }
        if !matches!(dest.scheme(), "http" | "https") {
            continue;
        }
        let label = element
            .text()
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if !label.is_empty() {
            deck.push(Block::Line(vec![Inline::Link {
                label,
                dest: dest.to_string(),
            }]));
        }
    }
    (!deck.blocks.is_empty()).then_some(deck)
}

/// Limit a WML card without cutting a tag or an escaped character in half.
pub fn limit_deck(deck: &mut gw_transcode::doc::Deck, max_bytes: usize) {
    while !deck.blocks.is_empty() && crate::wml::to_wml(deck).len() > max_bytes {
        deck.blocks.pop();
    }
}

/// Route secure links back through the gateway for server-side fetching.
pub fn route_secure_links(deck: &mut gw_transcode::doc::Deck) {
    use gw_transcode::doc::{Block, Inline};
    for block in &mut deck.blocks {
        match block {
            Block::Line(items) => {
                for item in items {
                    if let Inline::Link { dest, .. } = item {
                        if dest.starts_with("https://") {
                            let query = url::form_urlencoded::Serializer::new(String::new())
                                .append_pair("url", dest)
                                .finish();
                            *dest = format!("http://mw2.vzwwap.com/open?{query}");
                        }
                    }
                }
            }
            Block::Form(form) if form.action.starts_with("https://") => {
                // Use the path for the target, leaving the query free for GET
                // form fields (including duplicate names).
                let encoded: String =
                    url::form_urlencoded::byte_serialize(form.action.as_bytes()).collect();
                form.action = format!("http://mw2.vzwwap.com/submit/{encoded}");
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_portal() -> Portal {
        Portal::new(std::env::temp_dir().join(format!("portal-test-{}", uuid::Uuid::new_v4())))
            .unwrap()
    }
    fn session(portal: &Portal) -> String {
        portal.store.session(None).unwrap().0
    }
    #[test]
    fn observed_paths_and_bookmarks_are_scoped() {
        let p = test_portal();
        let a = session(&p);
        let b = session(&p);
        for u in [
            "http://mw2.vzwwap.com/",
            "http://mw2.vzwwap.com/goto",
            "http://mw2.vzwwap.com/search",
        ] {
            assert!(matches!(p.handle(&u.parse().unwrap(), &a), Action::Page(_)));
        }
        p.handle(&"http://mw2.vzwwap.com/addfavorite/?op=add&favname=Test&favurl=http%3A%2F%2Fexample.com%2F".parse().unwrap(), &a);
        let uri = "http://mw2.vzwwap.com/favorites".parse().unwrap();
        assert!(matches!(p.handle(&uri, &a), Action::Page(s) if s.contains("example.com")));
        assert!(matches!(p.handle(&uri, &b), Action::Page(s) if !s.contains("example.com")));
    }
    #[test]
    fn open_accepts_bare_host_and_escapes_bookmark() {
        let p = test_portal();
        let a = session(&p);
        assert!(
            matches!(p.handle(&"http://mw2.vzwwap.com/open?url=example.com".parse().unwrap(), &a), Action::Fetch(s) if s == "http://example.com")
        );
        p.handle(&"http://mw2.vzwwap.com/addfavorite/?favname=%3Cscript%3E&favurl=http%3A%2F%2Fexample.com".parse().unwrap(), &a);
        assert!(
            matches!(p.handle(&"http://mw2.vzwwap.com/favorites".parse().unwrap(), &a), Action::Page(s) if s.contains("&lt;script&gt;") && !s.contains("<script>"))
        );
    }
    #[test]
    fn search_uses_lite_and_secure_links_return_to_portal() {
        use gw_transcode::doc::{Block, Deck, Inline};
        let portal = test_portal();
        let ip = session(&portal);
        assert!(
            matches!(portal.handle(&"http://mw2.vzwwap.com/find?q=hello+world".parse().unwrap(), &ip), Action::Fetch(s) if s == "https://lite.duckduckgo.com/lite/?q=hello+world")
        );
        let mut deck = Deck::new();
        deck.push(Block::Line(vec![Inline::Link {
            label: "Test".into(),
            dest: "https://example.com/?a=1&b=2".into(),
        }]));
        route_secure_links(&mut deck);
        let Block::Line(items) = &deck.blocks[0] else {
            panic!()
        };
        let Inline::Link { dest, .. } = &items[0] else {
            panic!()
        };
        assert!(
            matches!(portal.handle(&dest.parse().unwrap(), &ip), Action::Fetch(s) if s == "https://example.com/?a=1&b=2")
        );
    }
    #[test]
    fn search_results_omit_controls_and_unwrap_redirects() {
        let html = r#"<form><select><option>language</option></select></form><a class="result-link" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fcat&amp;rut=abc">Cat</a>"#;
        let deck = search_results(html).unwrap();
        assert_eq!(deck.blocks.len(), 1);
        assert!(
            matches!(&deck.blocks[0], gw_transcode::doc::Block::Line(items) if matches!(&items[0], gw_transcode::doc::Inline::Link { dest, .. } if dest == "https://example.com/cat"))
        );
        assert!(search_results("<p>Try again</p>").is_none());
    }
    #[test]
    fn limit_preserves_well_formed_wml() {
        let mut deck = gw_transcode::doc::Deck::new();
        for _ in 0..100 {
            deck.push(gw_transcode::doc::Block::Line(vec![
                gw_transcode::doc::Inline::Text("long text ".repeat(20)),
            ]));
        }
        limit_deck(&mut deck, 1024);
        let output = crate::wml::to_wml(&deck);
        assert!(output.len() <= 1024);
        assert!(
            roxmltree::Document::parse_with_options(
                &output,
                roxmltree::ParsingOptions {
                    allow_dtd: true,
                    ..roxmltree::ParsingOptions::default()
                }
            )
            .is_ok()
        );
    }
    #[test]
    fn weather_requires_zip_and_remembers_location_per_client() {
        let portal = test_portal();
        let a = session(&portal);
        let b = session(&portal);
        assert!(
            matches!(portal.handle(&"http://mw2.vzwwap.com/weather/lookup?zip=10001".parse().unwrap(), &a), Action::WeatherLookup(s) if s == "10001")
        );
        assert!(
            matches!(portal.handle(&"http://mw2.vzwwap.com/weather/lookup?zip=bad".parse().unwrap(), &a), Action::Page(s) if s.contains("five-digit"))
        );
        assert!(matches!(
            portal.handle(
                &"http://mw2.vzwwap.com/weather/forecast?lat=40&lon=-74&name=New+York"
                    .parse()
                    .unwrap(),
                &a
            ),
            Action::WeatherForecast { .. }
        ));
        let home = "http://mw2.vzwwap.com/".parse().unwrap();
        assert!(
            matches!(portal.handle(&home, &a), Action::Page(s) if s.contains("Weather: New York"))
        );
        assert!(matches!(portal.handle(&home, &b), Action::Page(s) if !s.contains("New York")));
        assert!(matches!(
            portal.handle(
                &"http://mw2.vzwwap.com/weather/forecast?lat=NaN&lon=0"
                    .parse()
                    .unwrap(),
                &a
            ),
            Action::Page(_)
        ));
    }
    #[test]
    fn secure_form_actions_forward_get_fields_and_preserve_target_query() {
        use gw_transcode::doc::{Block, Deck, Form, FormMethod};
        let mut deck = Deck::new();
        deck.push(Block::Form(Form {
            action: "https://en.wikipedia.org/w/index.php?existing=one".into(),
            method: FormMethod::Get,
            fields: vec![],
            submit_label: "Search".into(),
        }));
        route_secure_links(&mut deck);
        let Block::Form(form) = &deck.blocks[0] else {
            panic!()
        };
        let uri = format!(
            "{}?title=Special%3ASearch&search=catgirl&search=cat",
            form.action
        )
        .parse()
        .unwrap();
        let portal = test_portal();
        let client = session(&portal);
        assert!(
            matches!(portal.handle(&uri, &client), Action::Fetch(s) if s == "https://en.wikipedia.org/w/index.php?existing=one&title=Special%3ASearch&search=catgirl&search=cat")
        );
    }
}
