# Application Icons

The root icon files are the Tauri 2 bundle inputs referenced by
`tauri.conf.json`:

- `32x32.png`, `128x128.png`, and `128x128@2x.png` cover Linux and desktop
  PNG consumers.
- `icon.icns` contains the macOS 11-15 icon family.
- `icon.ico` contains the Windows multi-size icon family.
- `Square*Logo.png`, `StoreLogo.png`, and `Wide310x150Logo.png` are Windows
  AppX and Microsoft Store assets.
- `app-icon.png` is the 1024 x 1024 Tauri source image.

Platform directories contain the corresponding complete asset sets:

| Directory | Contents |
| --- | --- |
| `android` | Legacy, round, adaptive foreground, Android 13 monochrome, XML resources, and Play Store icon. |
| `ios` | Xcode AppIcon set and Icon Composer source layers. |
| `linux` | Freedesktop hicolor PNG hierarchy plus scalable and symbolic SVGs. |
| `macos` | macOS 11-15 ICNS/iconset and macOS 26+ Icon Composer layers. |
| `tray` | Black, white, and SVG monochrome menu-bar or system-tray templates. |
| `windows` | Multi-frame ICO, PNG sizes, and AppX or Microsoft Store assets. |
| `source` | Original, generic, maskable, transparent, and monochrome SVG sources. |

Web favicon, Apple touch, and PWA assets live in `apps/desktop/public` because
Vite copies that directory directly into the frontend bundle.
