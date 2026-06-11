<div align="center">

<img src="res/logo-header.svg" alt="RustDesk Horizon" width="340">

# RustDesk Horizon

**Self-hosted remote desktop — with Virtual Display support**

A fork of [RustDesk](https://github.com/rustdesk/rustdesk) focused on one core addition: using a smartphone or tablet as a secondary monitor over the network, without any cloud dependency.

[![Fork](https://img.shields.io/badge/fork-rustdesk%2Frustdesk-blue?style=flat-square)](https://github.com/rustdesk/rustdesk)
[![Commits ahead](https://img.shields.io/badge/commits%20ahead-46-green?style=flat-square)](https://github.com/Rwanbt/rustdesk_horizon/commits/Dev)
[![Branch](https://img.shields.io/badge/branch-Dev-orange?style=flat-square)](https://github.com/Rwanbt/rustdesk_horizon/tree/Dev)
[![License](https://img.shields.io/badge/license-AGPL--3.0-red?style=flat-square)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.75+-000000?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![Flutter](https://img.shields.io/badge/Flutter-Dart-02569B?style=flat-square&logo=flutter)](https://flutter.dev/)

</div>

---

## What this fork adds

RustDesk is a solid self-hosted remote desktop solution. This fork extends it with **Virtual Display** — the ability to create a secondary monitor on the host machine, streamed live to a phone or tablet acting as a real second screen.

Everything else from RustDesk (relay server, P2P, E2E encryption, file transfer) remains unchanged.

---

## Platform support

| Platform | Driver | Status |
|---|---|---|
| **Windows** | IDD — Indirect Display Driver (`libs/virtual_display/`) | ✅ Stable |
| **Linux** | EVDI — kernel module + libevdi.so | 🧪 Experimental |
| **macOS** | CGVirtualDisplay API (14.6+) | ⚠️ Not yet tested |

> Linux support is functional on X11 but unstable on Wayland/GNOME. macOS builds compile but have not been validated on hardware. Contributions welcome.

---

## Added features (vs upstream)

### Virtual Display
- Cross-platform display driver abstraction (Windows IDD, Linux EVDI, macOS CGVirtualDisplay)
- Resolution picker UI directly in the Flutter toolbar
- Custom resolution support via `CUSTOM_VD_RESOLUTION`
- EDID generation (sRGB standard) for proper monitor detection
- `VdController` state machine managing display lifecycle (`Create / Deferred / Skipped`)

### UX & Mobile
- Auto-hiding toolbar on mobile — more usable screen real estate during sessions
- Low-latency defaults enabled out of the box: H264 codec, adaptive FPS, automatic resolution

### Infrastructure
- `hbb_common` forked as [`Rwanbt/hbb_common`](https://github.com/Rwanbt/hbb_common) and integrated as a submodule
- PowerShell build & Windows service install scripts (`build_and_install.ps1`)
- App identity fully renamed (Cargo.toml, EN/FR UI strings, service names)

---

## Tech stack

| Layer | Technology |
|---|---|
| Backend | Rust 1.75+ (Edition 2021) |
| UI | Flutter / Dart |
| FFI | flutter_rust_bridge |
| Video codecs | VP8/VP9 · H264 · H265 · hwcodec |
| Transport | WebRTC · TCP/UDP hole punching |
| Build | Docker · Cargo · PowerShell (Windows) |

---

## Getting started

This fork shares the same build process as upstream RustDesk. Follow the [official build guide](https://rustdesk.com/docs/en/dev/build/) for your platform, then clone this repo instead:

```sh
git clone --recurse-submodules https://github.com/Rwanbt/rustdesk_horizon
cd rustdesk_horizon
git checkout Dev
```

> `--recurse-submodules` is required — `hbb_common` is a forked submodule at `Rwanbt/hbb_common`.

### Windows (recommended — Virtual Display fully supported)

```sh
# After setting up Rust + vcpkg per the upstream guide:
.\build_and_install.ps1
```

### Linux

```sh
# Standard Rust + Flutter build (see upstream guide for distro-specific deps)
cargo run
```

---

## Repository structure (additions)

```
libs/
  virtual_display/            # IDD Windows driver
src/
  virtual_display_manager.rs  # Cross-platform VD orchestration
  platform/
    windows.rs                # IDD integration
    linux.rs                  # EVDI integration
    macos.rs                  # CGVirtualDisplay integration
build_and_install.ps1         # Windows build + service install script
```

---

## Roadmap

- [ ] EVDI stabilization on Linux Wayland / GNOME
- [ ] macOS hardware validation (CGVirtualDisplay)
- [ ] GPU cursor rendering fix (virtual display sessions)
- [ ] iOS client via SideStore (IPA sideloading)
- [ ] Video pipeline optimization for virtual display streams
- [ ] Sync with upstream RustDesk (182 commits behind master)

---

## Related

- [RustDesk](https://github.com/rustdesk/rustdesk) — upstream project
- [Rwanbt/hbb_common](https://github.com/Rwanbt/hbb_common) — forked common library used as submodule
- [rustdesk/rustdesk-server](https://github.com/rustdesk/rustdesk-server) — self-hosted relay/rendezvous server (unchanged, use upstream)

---

## Contributing

Issues and PRs welcome, especially for Linux Wayland and macOS validation.
For anything related to the base RustDesk functionality, please contribute upstream first.

---

<div align="center">

Forked from [rustdesk/rustdesk](https://github.com/rustdesk/rustdesk) · AGPL-3.0 License

</div>
