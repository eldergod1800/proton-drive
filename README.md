# Proton Drive

> ⚠️ **AI-Generated Software Notice**
> This application was written with the assistance of Claude (Anthropic AI). While it has been tested and is functional, it should be treated as experimental software. Use at your own risk, especially regarding credential handling and data integrity. The code has been reviewed by a human but was not written by a professional software team.

A native Linux desktop client for [Proton Drive](https://proton.me/drive), built for KDE Plasma. Provides a file browser, dark/light mode, tray icon, and file download — all communicating directly with the Proton Drive API using end-to-end encrypted sessions.

![Screenshot](Screenshot_20260328_085227.png)

---

## Features

- **Full login flow** — SRP authentication, CAPTCHA/human verification (GTK WebKit2 window), and TOTP 2FA support
- **Live file browser** — navigates your actual Proton Drive with clickable breadcrumb path bar
- **File download** — opens files with `xdg-open` after decrypting locally to `~/.cache/pdrive/`
- **Storage quota** — displays used/total storage in the sidebar
- **Dark & light mode** — persisted across sessions, toggle on both the login screen and main window
- **System tray** — close button minimizes to tray; app keeps running in the background
- **Daemon** — `pdrive-daemon` runs as a systemd user service and exposes a D-Bus interface (`org.protonmail.PDrive`) for status and sync operations

---

## Architecture

```
pdrive (GUI)                pdrive-daemon
    │                            │
    │  Slint UI (femtovg)        │  zbus D-Bus service
    │  tokio async runtime       │  (status only — file ops
    │                            │   handled directly in GUI)
    └─── pdrive-core ────────────┘
              │
              ├── DriveClient        (Proton Drive API)
              ├── TokenStore         (keyring session persistence)
              └── Config             (XDG config dir)
```

The GUI holds a live, authenticated `DriveClient` and performs all browse/download/quota operations in-process. The daemon is present for future sync functionality and provides a D-Bus status endpoint.

### Crates

| Crate | Purpose |
|-------|---------|
| `pdrive-gui` | Slint UI, login flow, tray icon |
| `pdrive-daemon` | D-Bus service, background sync (stub) |
| `pdrive-core` | `DriveClient`, `TokenStore`, `Config` |

---

## Dependencies

### Build
- Rust (stable toolchain)
- Cargo

### Runtime
| Package | Purpose |
|---------|---------|
| `dbus` | IPC between GUI and daemon |
| `python-gobject` | GTK3 Python bindings for CAPTCHA WebView |
| `webkit2gtk-4.1` | Renders Proton's human verification page |

---

## Building from Source

```bash
git clone https://github.com/eldergod1800/proton-drive.git
cd proton-drive
cargo build --release --bin pdrive --bin pdrive-daemon
```

Binaries are written to `target/release/`.

### Install manually

```bash
sudo install -Dm755 target/release/pdrive        /usr/bin/pdrive
sudo install -Dm755 target/release/pdrive-daemon  /usr/bin/pdrive-daemon
install -Dm644 assets/pdrive.desktop              ~/.local/share/applications/pdrive.desktop
install -Dm644 assets/icons/pdrive.svg            ~/.local/share/icons/hicolor/scalable/apps/pdrive.svg
systemctl --user enable --now pdrive.service
```

### Arch Linux (PKGBUILD)

```bash
makepkg -si
```

This will fetch sources, build, and install via `pacman`.

---

## Session Handling

Credentials are stored in the system keyring (via `libsecret`/KWallet). On startup the app attempts to restore the session from keyring. If the stored tokens have expired, it falls back to the login screen automatically.

**Security notes:**
- Passwords are held in memory using `zeroize` to reduce exposure
- Downloaded files are validated to stay within `~/.cache/pdrive/` before being opened
- No plaintext passwords are written to disk

---

## Known Limitations

- `pdrive-daemon` sync (upload/watch) is not yet implemented — stubs exist for future development
- The "Computers" (backup devices) section may fail to decrypt for some key configurations — this is a known issue with the underlying SDK and Proton's PGP key format
- Not affiliated with or endorsed by Proton AG

---

## Credits & Licenses

This project is licensed under the **GNU General Public License v3.0** — see [LICENSE](LICENSE).

The following MIT-licensed libraries make this possible:

- **[proton-sdk-rs2](https://github.com/tirbofish/proton-sdk-rs2)** by Thribhu K — Rust client for the Proton API including SRP authentication, session management, and Proton Drive operations. This project would not exist without this foundational work.
- **[proton-crypto-rs](https://github.com/ProtonMail/proton-crypto-rs)** by Proton AG — Cryptographic primitives (SRP, PGP) used internally by the SDK
- **[Slint](https://slint.dev/)** — UI framework
- **[zbus](https://gitlab.freedesktop.org/dbus/zbus)** — D-Bus IPC
- **[tokio](https://tokio.rs/)** — Async runtime

GPL-3.0 is compatible with the MIT license — the MIT-licensed dependencies retain their original licenses; GPL-3.0 applies to the original code in this repository.

---

## Contributing

Issues and pull requests welcome. This is a hobby project and the codebase is intentionally kept simple.
