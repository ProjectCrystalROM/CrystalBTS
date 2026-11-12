//! HTML-to-WML transcoding proxy.
//!
//! Runs beside Kannel in the packet-data gateway. wapbox is configured to fetch
//! through this process, so every page a WAP browser requests arrives here as an
//! ordinary forward-proxy request, is fetched and reduced to a deck, and goes
//! back as WML for wapbox to compile to WBXML. Content a handset renders on its
//! own goes back exactly as the site sent it.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use clap::Parser;
use gw_transcode::doc::{Deck, notice_deck};
use gw_transcode::proxy::{
    FetchBody, MAX_BODY_BYTES, Proxy, ProxyError, forwardable_request_headers,
    forwardable_response_headers,
};
use gw_transcode::transcode::{self, Limits};
use tracing::{info, warn};

use wap_gw::is_handset_native;
use wap_gw::portal::{Action, Portal};
use wap_gw::wml::{CTYPE_WML, to_wml};

/// Default upstream fetch timeout.
const DEFAULT_TIMEOUT_SECS: u64 = 20;

/// Longest reason shown on a handset. A card holds a few short lines, so a long
/// cause is trimmed rather than pushing the rest of the notice off the screen.
const MAX_REASON_CHARS: usize = 120;

/// Flatten an error and its causes into one line. The outermost message reads
/// the same whether the name did not resolve, the connection was refused, the
/// server never answered or the certificate failed to verify, so the cause is
/// the only part worth logging.
fn causes(e: &dyn std::error::Error) -> String {
    let mut out = e.to_string();
    let mut src = e.source();
    while let Some(cause) = src {
        out.push_str(": ");
        out.push_str(&cause.to_string());
        src = cause.source();
    }
    out
}

/// The innermost cause, which is the one that names what actually went wrong.
fn reason(e: &dyn std::error::Error) -> String {
    let mut deepest = e;
    while let Some(cause) = deepest.source() {
        deepest = cause;
    }
    let text = deepest.to_string();
    match text.char_indices().nth(MAX_REASON_CHARS) {
        Some((cut, _)) => format!("{}...", &text[..cut]),
        None => text,
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "wap-gw",
    about = "HTML-to-WML transcoding proxy for the WAP gateway"
)]
struct Args {
    /// Address to serve the forward proxy on.
    #[arg(long, env = "WAP_GW_BIND", default_value = "127.0.0.1:8090")]
    bind: SocketAddr,

    /// Maximum blocks emitted for one page. Defaults to the transcoder's
    /// shared cap. WML carries more per-element overhead than HDML, so lower
    /// it here if decks come out too large for the handset.
    #[arg(long, env = "WAP_GW_MAX_BLOCKS", default_value_t = Limits::default().max_blocks)]
    max_blocks: usize,

    /// Maximum input fields carried from one form.
    #[arg(long, env = "WAP_GW_MAX_FIELDS", default_value_t = Limits::default().max_fields)]
    max_fields: usize,

    /// Serve the Verizon carrier portal for HTTP-proxy handsets.
    #[arg(long, env = "WAP_GW_CARRIER_PORTAL", default_value_t = false)]
    carrier_portal: bool,

    /// Render local portal pages as WML for Kannel's WAP client.
    #[arg(long, env = "WAP_GW_PORTAL_WML", default_value_t = false)]
    portal_wml: bool,

    /// Directory holding persistent carrier portal sessions.
    #[arg(
        long,
        env = "WAP_GW_PORTAL_STORE",
        default_value = "/var/lib/1xbts/portal"
    )]
    portal_store: PathBuf,

    /// Upstream fetch timeout.
    #[arg(long, env = "WAP_GW_TIMEOUT_SECS", default_value_t = DEFAULT_TIMEOUT_SECS)]
    timeout_secs: u64,

    /// User-Agent presented to the origin server when the handset's request
    /// carries none.
    #[arg(
        long,
        env = "WAP_GW_USER_AGENT",
        default_value = "Mozilla/5.0 (compatible; 1xBTS WAP gateway)"
    )]
    user_agent: String,
}

struct AppState {
    proxy: Proxy,
    limits: Limits,
    portal: Option<Portal>,
    portal_wml: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    let limits = Limits {
        max_blocks: args.max_blocks,
        max_fields: args.max_fields,
        ..Limits::default()
    };
    let state = Arc::new(AppState {
        proxy: Proxy::new(&args.user_agent, Duration::from_secs(args.timeout_secs))?,
        limits,
        portal_wml: args.portal_wml,
        portal: if args.carrier_portal {
            Some(Portal::new(&args.portal_store)?)
        } else {
            None
        },
    });

    let app = Router::new().fallback(handle).with_state(state);
    let listener = tokio::net::TcpListener::bind(args.bind).await?;
    info!(
        bind = %args.bind,
        max_blocks = limits.max_blocks,
        "wap-gw: HTML->WML proxy listening"
    );
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

async fn handle(State(state): State<Arc<AppState>>, req: Request) -> Response {
    let (parts, body) = req.into_parts();
    let method = parts.method;
    let mut uri = parts.uri.to_string();
    let mut issued_cookie: Option<String> = None;
    let mut issued_wap_session: Option<String> = None;
    let mut portal_fetch = false;
    if let Some(portal) = &state.portal {
        let host = parts.uri.host().unwrap_or("");
        if host.eq_ignore_ascii_case("homepage") {
            return (
                StatusCode::FOUND,
                [
                    (header::LOCATION, "http://mw2.vzwwap.com/"),
                    (header::CACHE_CONTROL, "no-store"),
                ],
            )
                .into_response();
        }
        if host.eq_ignore_ascii_case("mw2.vzwwap.com") {
            let session_cookie = parts
                .headers
                .get_all(header::COOKIE)
                .iter()
                .filter_map(|value| value.to_str().ok())
                .flat_map(|value| value.split(';'))
                .filter_map(|part| part.trim().split_once('='))
                .find_map(|(name, value)| (name == "kozume_session").then_some(value));
            // Kannel 1.4 does not encode Set-Cookie in connectionless WSP.
            // A WAP client can carry the same session ID as an application header.
            let session_cookie = session_cookie.or_else(|| {
                state
                    .portal_wml
                    .then(|| parts.headers.get("x-kozume-session"))
                    .flatten()
                    .and_then(|value| value.to_str().ok())
            });
            let (session, fresh) = match portal.store.session(session_cookie) {
                Ok(session) => session,
                Err(error) => {
                    warn!(%error, "portal session unavailable");
                    return (
                        StatusCode::SERVICE_UNAVAILABLE,
                        "Portal storage unavailable",
                    )
                        .into_response();
                }
            };
            if fresh {
                if state.portal_wml {
                    issued_wap_session = Some(session.clone());
                }
                let expires = httpdate::fmt_http_date(
                    std::time::SystemTime::now() + Duration::from_secs(31_536_000),
                );
                issued_cookie = Some(format!(
                    "kozume_session={session}; Path=/; Max-Age=31536000; Expires={expires}; HttpOnly"
                ));
            }
            let action = portal.handle(&parts.uri, &session);
            let page = match action {
                Action::Page(page) => Some(page),
                Action::Redirect => {
                    return (
                        StatusCode::FOUND,
                        [(header::LOCATION, "http://mw2.vzwwap.com/")],
                    )
                        .into_response();
                }
                Action::Unavailable => {
                    return (
                        StatusCode::SERVICE_UNAVAILABLE,
                        "Portal storage unavailable",
                    )
                        .into_response();
                }
                Action::Fetch(target) => {
                    uri = target;
                    portal_fetch = true;
                    None
                }
                Action::WeatherLookup(zip) => {
                    Some(wap_gw::weather::lookup(&state.proxy, &zip).await)
                }
                Action::WeatherForecast {
                    latitude,
                    longitude,
                    name,
                } => {
                    Some(wap_gw::weather::forecast(&state.proxy, latitude, longitude, &name).await)
                }
                Action::Pass => None,
            };
            if let Some(page) = page {
                let mut response = if state.portal_wml {
                    let deck =
                        transcode::html_to_deck(&page, "http://mw2.vzwwap.com/", &state.limits);
                    wml_response(StatusCode::OK, &deck, None)
                } else {
                    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], page).into_response()
                };
                response.headers_mut().insert(
                    header::CACHE_CONTROL,
                    header::HeaderValue::from_static("no-store"),
                );
                set_session_headers(
                    &mut response,
                    issued_cookie.as_deref(),
                    issued_wap_session.as_deref(),
                );
                return response;
            }
        }
    }
    let mut req_headers = forwardable_request_headers(&parts.headers);
    req_headers.remove("x-kozume-session");
    if portal_fetch {
        req_headers.remove(header::COOKIE);
    }

    // A forward proxy is addressed in absolute form. Anything else is a client
    // that reached us directly and has no page to name.
    if !(uri.starts_with("http://") || uri.starts_with("https://")) {
        return session_response(
            wml_response(
                StatusCode::BAD_REQUEST,
                &notice_deck("Bad request", "This is a proxy; ask for a full URL."),
                None,
            ),
            issued_cookie.as_deref(),
            issued_wap_session.as_deref(),
        );
    }

    let page = match method {
        Method::GET => state.proxy.get(&uri, &req_headers).await,
        Method::POST => {
            let bytes = match axum::body::to_bytes(body, MAX_BODY_BYTES).await {
                Ok(b) => b,
                Err(e) => {
                    // Submitting the form without its fields would look like a
                    // server-side failure to the handset, so say so instead.
                    warn!(%uri, error = %e, "wap-gw: unreadable request body");
                    return session_response(
                        wml_response(
                            StatusCode::OK,
                            &notice_deck("Too large", "That submission was too big to send."),
                            None,
                        ),
                        issued_cookie.as_deref(),
                        issued_wap_session.as_deref(),
                    );
                }
            };
            state.proxy.post(&uri, &req_headers, bytes.to_vec()).await
        }
        other => {
            warn!(%other, %uri, "wap-gw: unsupported method");
            return session_response(
                wml_response(
                    StatusCode::METHOD_NOT_ALLOWED,
                    &notice_deck("Not supported", "That request type is not supported."),
                    None,
                ),
                issued_cookie.as_deref(),
                issued_wap_session.as_deref(),
            );
        }
    };

    let page = match page {
        Ok(page) => page,
        Err(e) => {
            warn!(%uri, error = %causes(&e), "wap-gw: fetch failed");
            let deck = match e {
                ProxyError::TooLarge(_) => {
                    notice_deck("Too large", "That page is too big for the gateway to load.")
                }
                ProxyError::Blocked(_) => {
                    notice_deck("Not allowed", "That address is not reachable from here.")
                }
                _ => notice_deck(
                    "Fetch failed",
                    &format!("Could not load the page. {}", reason(&e)),
                ),
            };
            // A fetch failure is still a deck: the handset renders the notice,
            // which is more use than the browser's own error card for a status
            // it cannot explain.
            return session_response(
                wml_response(StatusCode::OK, &deck, None),
                issued_cookie.as_deref(),
                issued_wap_session.as_deref(),
            );
        }
    };

    info!(
        %uri,
        final_url = %page.final_url,
        status = page.status,
        ctype = %page.content_type,
        bytes = page.bytes.len(),
        "wap-gw: fetched"
    );

    if is_handset_native(&page.content_type) {
        let status = StatusCode::from_u16(page.status).unwrap_or(StatusCode::BAD_GATEWAY);
        let mut origin_headers = page.headers.clone();
        if portal_fetch {
            origin_headers.remove(header::SET_COOKIE);
        }
        let mut response = with_origin_headers(
            (status, page.bytes.clone()).into_response(),
            &origin_headers,
            &page.content_type,
        );
        set_session_headers(
            &mut response,
            issued_cookie.as_deref(),
            issued_wap_session.as_deref(),
        );
        return response;
    }

    let mut deck = match page.body() {
        FetchBody::Html(html) => {
            if state.portal.is_some()
                && page
                    .final_url
                    .starts_with("https://lite.duckduckgo.com/lite/")
            {
                wap_gw::portal::search_results(&html).unwrap_or_else(|| {
                    transcode::html_to_deck(&html, &page.final_url, &state.limits)
                })
            } else {
                transcode::html_to_deck(&html, &page.final_url, &state.limits)
            }
        }
        FetchBody::Text(text) => transcode::text_to_deck(&text, "Page", &state.limits),
        FetchBody::Other => notice_deck(
            "Not shown",
            &format!(
                "This page is {}, which a phone cannot show.",
                page.content_type
            ),
        ),
    };
    if state.portal.is_some() {
        wap_gw::portal::route_secure_links(&mut deck);
        wap_gw::portal::limit_deck(&mut deck, 8192);
    }
    if state.portal.is_some() {
        let mut headers = HeaderMap::new();
        if !portal_fetch {
            for value in page.headers.get_all(header::SET_COOKIE) {
                headers.append(header::SET_COOKIE, value.clone());
            }
        }
        let mut response = wml_response(StatusCode::OK, &deck, Some(&headers));
        set_session_headers(
            &mut response,
            issued_cookie.as_deref(),
            issued_wap_session.as_deref(),
        );
        response
    } else {
        wml_response(StatusCode::OK, &deck, Some(&page.headers))
    }
}

fn session_response(
    mut response: Response,
    cookie: Option<&str>,
    wap_session: Option<&str>,
) -> Response {
    set_session_headers(&mut response, cookie, wap_session);
    response
}

fn set_session_headers(response: &mut Response, cookie: Option<&str>, wap_session: Option<&str>) {
    if let Some(value) = cookie.and_then(|s| header::HeaderValue::from_str(s).ok()) {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    if let Some(value) = wap_session.and_then(|s| header::HeaderValue::from_str(s).ok()) {
        response.headers_mut().insert("x-kozume-session", value);
    }
}

/// A WML reply. `origin` carries the fetched page's headers on to the handset
/// (cookies, caching), with the content type replaced by the deck's own.
fn wml_response(status: StatusCode, deck: &Deck, origin: Option<&HeaderMap>) -> Response {
    let resp = (status, to_wml(deck)).into_response();
    match origin {
        Some(h) => with_origin_headers(resp, h, CTYPE_WML),
        None => with_origin_headers(resp, &HeaderMap::new(), CTYPE_WML),
    }
}

fn with_origin_headers(mut resp: Response, origin: &HeaderMap, content_type: &str) -> Response {
    let headers = resp.headers_mut();
    for (name, value) in &forwardable_response_headers(origin) {
        headers.append(name.clone(), value.clone());
    }
    match header::HeaderValue::from_str(content_type) {
        Ok(v) => {
            headers.insert(header::CONTENT_TYPE, v);
        }
        Err(_) => {
            headers.remove(header::CONTENT_TYPE);
        }
    }
    resp
}
