# Upstream cherry-picks from 1xBTS (Airave 2.5 deployment)

Branch: `airave-2.5-upstream-fixes`, based on `a3f89e1` (CrystalBTS `main`).
Upstream: `chrismoos/1xbts` `main` @ `48155e6`.
Shared base commit: `9897714` (CrystalBTS diverged here, so everything below is
new upstream work that CrystalBTS did not have).

These are all **protocol/SDR/timing** fixes — none of them touch the femtocell
(Airave) code path, so they apply the same way whether the deployment is an
Airave or a standard SDR. They were selected because they make the shared core
that the Airave's 1x + EV-DO traffic runs through faster and more stable.

## Applied (10 commits, all clean or CHANGELOG-only conflicts)

| # | Upstream | Commit here | Why it matters for Airave 2.5 |
|---|----------|-------------|-------------------------------|
| 1 | `67366c2` | `06abe93` | Build fix for macOS 27 — only needed if you build on macOS. |
| 2 | `5c984fe` | `68f3517` | **BTS syncs hardware time on its own thread.** Stops the TX loop from blocking on `get_hardware_time()` on every batch, which removes timing jitter. On a femtocell the BTS clock drives the Abis/air-interface frame boundary, so jitter shows up as traffic-channel FER and dropped calls. |
| 3 | `4905eaf` | `ac6457d` | PlutoSky (Zynq 7020) radio config — inert unless you use that hardware. |
| 4 | `1b603ef` | `fd36d65` | **cdma-ms softphone Status Response handling** (MS library, not the femtocell). Needed if you test against the stack with the softphone. Adds `RadioPipeHandle::try_inject_rx` used by the MS. |
| 5 | `0ea3ca0` | `392452b` | Test fix for the fake-SDR registration test (serializes real-time test pairs, adds MEID). |
| 6 | `05de79a` | `aa5a4f4` | **HLR race when a new mobile registers with MEID.** `INSERT ... ON CONFLICT DO NOTHING` + re-select/update instead of a hard failure. Airave deployments commonly register handsets with MEID, so first-time registrations could be rejected under concurrency. |
| 7 | `16ff4ff` | `5c3fb9c` | **Biggest one: CPU load reduction** on 1x and composite 1x+EV-DO — alloc-free TX pulse shaping, HRPD access correlator reuse/dedup, PN-LC correlator no longer `drain()`s per window, reverse traffic despread into reusable buffers, DRAC/ACK decoders reuse Walsh decoders, shared FFT planner, scheduler log gating, one-X matched filter output reuse, bladeRF `num_buffers` 16 → 32 (RX overflow reduction), startup lookahead so the first timed TX doesn't miss its deadline, `target-cpu=native`. Also fixes the HRPD A8 RLP sequence accounting in `cdma-an`. |
| 8 | `83766af` | `0206856` | sdr-hal 0.1.0 → 0.1.4 (lockfile). This is the crate that carries the `RadioClock` used by #2. |
| 9 | `5c45e39` | `20e80a7` | Follow-up test fix that matches the sdr-hal 0.1.4 `serve()` signature. |
| 10 | `48155e6` | `6276e4e` | CI fix: pin `RUSTFLAGS` in CI so cached artifacts don't run `target-cpu=native` binaries on a different CPU. |

Conflicts encountered: **only `CHANGELOG.md`** (both repos keep an unreleased
`# HEAD` list). Resolved by keeping CrystalBTS's entries and appending the
upstream lines. No Rust source conflict in any of the ten.

## Deliberately NOT cherry-picked

* Nothing else exists: `9897714..48155e6` is exactly the ten commits above, and
  all ten are applied. Upstream has no femtocell/Airave work newer than the
  shared base.

## Caveats for an Airave 2.5 box

* **Build happens on the deployment box (Linux).** The machine used to prepare
  this branch is a Windows box that neither compiles nor runs the stack, so
  "not built here" is expected, not a gap in the work.
* **`target-cpu=native`** (from #7, in `.cargo/config.toml`) is the right
  setting for that arrangement: the binary is tuned to the CPU it is built on,
  and you build on the box that runs it. Only if you ever build on one machine
  and deploy to another do you need to drop the flag, or the target gets
  `SIGILL` on an instruction the build host had and it doesn't. CI overrides it
  already (`48155e6`).
* **sdr-hal version.** `5c984fe` and `16ff4ff` need `RadioClock` /
  `clock_reader()` from sdr-hal **0.1.4**; the `Cargo.lock` bump (`83766af`)
  covers the normal case. If the build box has its own `[patch]` table or a
  vendored older sdr-hal, those two commits fail with "no method named
  `clock_reader`" — check `cargo check -p cdma-bts` first.
* **`config/radio_bladerf_micro2.json`** `num_buffers: 32` only affects bladeRF;
  it is harmless if you are femtocell-only.
* None of these commits touch `config/femto.json`, `crates/cdma-femto`, or the
  Airave 2.5 profile (`airave-25-2x` / `airave-25-5x`). CrystalBTS's own Airave
  fixes (term_auth_psk resync, A11 MSID resolution, OTASP correlation, UA
  profile serving) are untouched.
* Rebuild everything, including the web UI if you use Docker: `docker compose
  up -d --build`.
