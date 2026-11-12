# CrystalBTS

[![CI](https://github.com/ProjectCrystalROM/CrystalBTS/actions/workflows/ci.yml/badge.svg)](https://github.com/ProjectCrystalROM/CrystalBTS/actions/workflows/ci.yml)
[![Upstream](https://img.shields.io/badge/upstream-1xBTS-7A5AF8.svg)](https://github.com/chrismoos/1xbts)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024-orange.svg)](Cargo.toml)

CrystalBTS is an experimental CDMA2000 1x and HRPD (1xEV-DO) radio-access and
core-network stack implemented primarily in Rust. It includes BTS, BSC, HRPD
AN, MSC, HLR, SMSC, packet data, voice, SDR radio backends, and a web dashboard
for development and operation.

CrystalBTS is a hard fork of [1xBTS](https://github.com/chrismoos/1xbts),
originally created by Chris Moos. It preserves the upstream project's
attribution and Apache-2.0 licensing while pursuing its own compatibility and
revival work.

> **New: EV-DO is here.** Initial HRPD support provides end-to-end packet data
> for EV-DO Rev 0 and Rev A devices, including the air interface, AN
> session/access/traffic handling, the A8/A9/A10/A11 packet path, 1x
> coordination, and dashboard visibility. Run 1x and EV-DO on adjacent
> carriers from one SDR, or run an HRPD-only carrier.

This project is intended for research, interoperability testing, and lab use.
Operate RF hardware only where you are authorized to transmit. You have been warned.

<a href="docs/images/screenshot.png">
  <img src="docs/images/screenshot.png" alt="CrystalBTS dashboard" width="720">
</a>

## Why CrystalBTS

While CDMA2000 was a major technology in the US and some other places around the world, 
it was never as ubiquitious as other standards like GSM. Up until now there has never been
a way to revive old CDMA2000 devices. CrystalBTS continues that work with an
independent direction for legacy-device compatibility and experimentation.

## Status

CrystalBTS is under active development. The current stack can run as a
network-in-a-box process for local integration, or as separate network-node
processes. The 1x BTS and BSC can also run split over the included Abis
transport. Support varies by handset, radio backend, channel configuration, and
service option.

SMS, voice, and packet data are working on 1x. Initial EV-DO Rev 0 and Rev A
packet-data support is also working, but remains new and experimental. There is
still much work to be done for production hardening. It is still a lot of fun,
though.

## Architecture

The Rust workspace keeps first-party crates flat under `crates/` and external
integration wrappers under `external/`:

- `crates/` - CDMA protocol, RAN, core-network, packet, voice, and tooling crates.
- `external/` - SIP/native and other outside-library wrapper crates.
- `1xbts-web` - Next.js dashboard backed by the management gRPC APIs.
- `proto` - protobuf service definitions.

Project documentation lives in this repository's [`docs/`](docs/) directory.
The [upstream 1xBTS documentation](https://1xbts.org/docs) remains useful for
features CrystalBTS has not yet documented separately.

### Network Nodes

The stack implements the 3GPP2 1x and HRPD reference paths, with each node able
to run as its own process and communicate over standard reference points:

- **BTS** (`cdma-bts`) - 1x and HRPD air-interface PHY/MAC, driven by an SDR. Hosts the HRPD Access Network (sessions, UATI assignment, A8/A9 to the PCF) when EV-DO is enabled.
- **BSC** (`cdma-bsc`) - 1x radio resource management for every enrolled BTS, speaking Abis to each.
- **MSC** (`cdma-msc`) - circuit-switched core. Dials A1 to every base station it is configured with.
- **HLR** (`cdma-hlr`) - subscriber database (PostgreSQL-backed).
- **SMSC** (`cdma-smsc`) - short message service center.
- **PCF** (`cdma-pcf`) - packet control function; HRPD A8/A9 to the AN and
  A10/A11 to the PDSN.
- **PDSN** (`cdma-pdsn`) - packet data serving node, FoU/TUN packet path.
- **voice-gw** (`cdma-voice-gw`) - SIP gateway for outbound voice calls (PSTN origination). See [voice gateway setup](https://1xbts.org/docs/guides/voice-gateway/) for trunk + STUN configuration.
- **NIB** (`cdma-nib`) - network-in-a-box launcher that builds every network element and runs each as a child process.

## Prerequisites

At minimum:

- Rust toolchain with Cargo.
- PostgreSQL for HLR/SMSC state (`docker compose up -d postgres` starts a local `1xbts` database on port 45432).
- `protoc` for protobuf code generation (Ubuntu/Debian: `protobuf-compiler`).
- `pkg-config` and a C compiler for native bindings.
- `libclang` for bindgen (Ubuntu/Debian: `libclang-dev`).

Optional radio and voice dependencies depend on enabled features and hardware:
UHD/USRP, LimeSuite, SoapySDR, bladeRF, and Baresip libre/re.

### Ubuntu / Debian

Common packages (always required):

```sh
sudo apt-get install -y \
    build-essential pkg-config protobuf-compiler libprotobuf-dev libssl-dev \
    libclang-dev libre-dev
```

On Ubuntu, `libre-dev` may be too old for the voice gateway's SIP registration support. Install the current libre version described in the Building from Source guide before building the core.

Then add the packages for whichever SDR backend(s) you plan to enable:

**bladeRF** (`--features bladerf-backend`):

```sh
sudo apt-get install -y libbladerf-dev
```

**UHD / USRP** (`--features uhd-backend`):

```sh
sudo apt-get install -y libuhd-dev
```

**LimeSDR** (`--features lime-backend`):

```sh
sudo apt-get install -y liblimesuite-dev
```

### Tested SDR Hardware

- bladeRF Micro 2.0
- Ettus USRP B210 (UHD)
- LimeSDR Mini v2

Other SoapySDR-compatible devices may work but are not regularly exercised.

### Tested Handsets

- Apple iPhone (CDMA)
- Motorola V60s
- Qualcomm QCP-860
- Nokia Lumia 735
- Samsung SCH-U340
- BlackBerry 8830

## Quick Start

Clone, bring up the support services, and run the network-in-a-box:

```sh
git clone git@github.com:ProjectCrystalROM/CrystalBTS.git
cd CrystalBTS

# Linux:
sudo modprobe fou ipip   # load kernel modules required by fou-nat
docker compose up -d --build

# macOS (uses bridge networking + host.docker.internal):
docker compose -f docker-compose.yml -f docker-compose.macos.yml up -d --build

# postgres :45432 · dashboard :3000 · fou-nat :17012 · speed test :5656
cargo run --release -p cdma-nib --no-default-features --features bladerf-backend -- \
    --config-dir config \
    --radio-config config/radio_bladerf_micro2.json
```

EV-DO is disabled by default. The checked-in BTS configuration already contains
an example HRPD channel and sector definition; enable it without modifying the
base file by creating `config/bts.local.json`:

```json
{
  "evdo": {
    "enabled": true
  }
}
```

The default `composite` mode places the 1x and EV-DO carriers in one SDR
passband and advertises the HRPD neighbor on 1x. The BTS derives the required
sample rate and bandwidth from the two configured channels. To transmit only
the EV-DO carrier, also set `"mode": "hrpd_only"`. Review the channel, SectorID,
subnet mask, color code, gain, and your SDR's available bandwidth before
transmitting.

Default service ports are listed in `docs/PORTS.md`. If port 3000 is already in
use, set `ONEXBTS_WEB_PORT`, for example `ONEXBTS_WEB_PORT=3001 docker compose up 1xbts-web`.
Use `--build` after pulling repo updates so Compose rebuilds images that copy
local files, such as `fou-nat` and `speedtest`.

### Femtocell Profiles

For `airave-25-5x`, set `a11_security.spi` and
`a11_security.shared_secret_hex` in `config/pdsn.local.json`. Provisioning uses
these values for both 1x and EV-DO. Restart the PDSN and `femto-core`, then
sync the femtocell after changing them.

Femtocell containers are disabled during a normal `docker compose up`. Both
profiles use `config/femto.json` and its `config/femto.local.json` overlay for
the SeGW and femtocell core. Create the local overlay from the example:

```sh
cp config/femto.local.json.example config/femto.local.json
```

Configure each FAP under `sites` in the local overlay, including its identity,
credentials, and radio settings. The SeGW connections are derived from those
sites. The `segw` block controls the FAP address pool, tunnel DNS address, and
generated StrongSwan configuration directory.

Start the supporting containers and femtocell edge, including StrongSwan,
ACS, SIP/FCS, packet ingress, and the management proxy, with:

```sh
docker compose --profile femto up -d --build
```

Compose does not start the shared network core. For a femtocell-only
installation, follow the Femtocell Setup guide to add the femtocell to the MSC,
configure the PDSN and host return route, and run the shared core without an SDR.

For Airave 2.5+, use `airave-25plus`. For the original Airave 2.5, use
`airave-25-2x` for 2.x firmware or `airave-25-5x` for 5.x firmware.

To terminate FAP IPsec on this host while running the femtocell core elsewhere,
start only StrongSwan, tunnel DNS, and the SeGW configuration controller:

```sh
docker compose --profile femto-segw-only up -d --build femto-segw
```

The SeGW-only profile assigns FAP virtual addresses and terminates the tunnel.
Routing decrypted traffic to a remote core is deployment-specific.
The packet-data speed test is available on the host at `http://localhost:5656`
by default, and from mobile packet-data clients at `http://speed/` or
`http://speed.local.1xbts.org/`.

To customize a config without editing the checked-in defaults, drop a sibling
`<name>.local.json` next to it (e.g. `config/bts.local.json`). The loader
deep-merges the local file on top of the base before validation. `*.local.json`
is gitignored. See the [Configuration guide](https://1xbts.org/docs/getting-started/configuration/#local-overrides).

The stock BTS configuration uses Verizon settings. To try Sprint, add
`--bts-profile sprint`. See [Carrier Profiles](https://1xbts.org/docs/carrier-profiles/overview/)
for available profiles and local overrides.

Packet-data DNS advertised to mobiles is configured in `config/pdsn.json` under
`packet.primary_dns` and `packet.secondary_dns`; the checked-in defaults point to
the FOU gateway resolver so `speed` and `speed.local.1xbts.org` resolve locally.

### SDR Backend Features

Enable exactly one SDR backend at build time:

| Ubuntu package     | Hardware            | Cargo feature                |
| ------------------ | ------------------- | ---------------------------- |
| `libbladerf-dev`   | bladeRF devices     | `--features bladerf-backend` |
| `libuhd-dev`       | USRP B200/B210      | `--features uhd-backend`     |
| `liblimesuite-dev` | LimeSDR devices     | `--features lime-backend`    |

### Tests

Run focused tests while developing:

```sh
cargo test -p cdma-bts
cargo test -p cdma-bsc
cargo test -p cdma-msc
```

### Web Dashboard

The dashboard ships in `1xbts-web` (Next.js) and is started by `docker compose up`
above. To run it directly:

```sh
cd 1xbts-web
npm install -g pnpm
pnpm install
pnpm dev
```

## Support

- **GitHub Discussions** — [ProjectCrystalROM/CrystalBTS discussions](https://github.com/ProjectCrystalROM/CrystalBTS/discussions) for questions, ideas, and show-and-tell.
- **GitHub Issues** — [ProjectCrystalROM/CrystalBTS issues](https://github.com/ProjectCrystalROM/CrystalBTS/issues) for bug reports and feature requests.
- **Upstream community** — `#1xbts` on [Libera.Chat](https://libera.chat) remains an upstream community channel.

## License

CrystalBTS is a derivative of 1xBTS and is licensed under Apache-2.0. The SDR wrapper crates
under `external/` (`bladerf`, `bladerf-sys`, `limesuite`, `limesuite-sys`, `uhd`,
`uhd-sys`) are dual-licensed `MIT OR Apache-2.0`; see the individual `Cargo.toml`
files for package-level license metadata.

The optional native Baresip libre/re dependency used by the SIP voice gateway is
provided by the system and is distributed upstream under BSD-3-Clause terms.
