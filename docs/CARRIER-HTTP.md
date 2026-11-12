# Carrier HTTP proxy portal

Some Verizon UP.Browser phones use an HTTP proxy and request the carrier portal
at `http://homepage/` and `http://mw2.vzwwap.com/`. To enable the experimental
portal, set `CARRIER_HTTP_ENABLE=1` in the Compose environment and rebuild and
recreate `fou-nat` and `carrier-portal`. The portal runs as the explicit
`carrier-portal` Compose sidecar and shares the packet gateway's tunnel network
namespace. The default redirect covers `69.78.235.46:8080`; set
`CARRIER_HTTP_HIJACK_ENDPOINTS` to a comma-separated list of IP:port pairs if a
phone uses other proxy addresses. The redirect applies to all packet clients
that connect to those endpoints.

The portal provides Go to URL, DuckDuckGo Lite search, Wikipedia, US ZIP-code
weather and the observed add-favorite path. The `http://homepage/` address
redirects to `http://mw2.vzwwap.com/`, where a persistent browser cookie identifies
the session. Bookmarks and the selected weather location live in the Compose
volume `carrier-portal-state` and survive container recreation. Clearing browser
cookies creates a new session; settings from the earlier in-memory, IP-keyed
portal cannot be migrated after that process is stopped.

When the carrier portal is enabled, it is also available through the WAP
transcoder. Kannel's connectionless WSP path does not transmit `Set-Cookie`, so
WAP clients must save the `X-Kozume-Session` application header from the first
portal response and send that same header on later requests to the portal. This
is the same session identifier as the HTTP cookie; it is not a browser IP.
Weather uses Open-Meteo and GeoNames.

External pages are fetched and reduced to WML using the WAP transcoder. Cards
are capped at 8 KiB; longer pages are shortened and do not yet have pagination. Entered HTTPS URLs and HTTPS links in transcoded pages are fetched by the gateway; the proxy does not terminate HTTPS or
implement HTTP CONNECT, so direct HTTPS navigation from the phone may not
work. Opera Micro/Opera Mini clients need a separate protocol and are not
supported by this portal.
