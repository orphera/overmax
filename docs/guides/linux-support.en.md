# Linux Support Guide

Linux support for Overmax is still in its early stages. It is not as broadly supported as Windows; only Proton/XWayland environments that meet the conditions below are currently supported.

## Support Scope

| Game mode / environment | Support status | What to expect |
| --- | --- | --- |
| Borderless fullscreen | **Supported and recommended** | Requires the environment listed below. |
| Windowed mode | **Usable, but effectively unsupported** | Capture and recognition can work on a single output, but correct operation is not guaranteed. Place the overlay manually; it does not follow the game window. A fixed window size that changes the game's aspect ratio can cause recognition to fail. |
| Exclusive fullscreen | Limited validation | Compatibility is not guaranteed. Use borderless fullscreen. |
| GNOME (Mutter) | Unsupported | The overlay requires `wlr-layer-shell`, which Mutter does not support. |
| Gamescope / Steam Deck Gaming Mode | Unsupported | Window tracking, capture, and overlay integration in Gamescope's separate XWayland session are not currently supported or validated. |

### Requirements

- x86_64 Linux with glibc 2.35 or later
- A Wayland compositor that supports `wlr-layer-shell`
- XWayland running in the same session
- Vulkan drivers
- fontconfig and a font with Korean glyphs
- DJMAX RESPECT V running through Proton/XWayland on the same `DISPLAY`

**Use a Wayland desktop session and run the game through XWayland in that session.** X11 desktop sessions and games running through native Wayland are not supported.

## Verified Environments

- Release bundle build and library compatibility baseline: Ubuntu 22.04 / glibc 2.35. This does not imply overlay support on Ubuntu's default GNOME desktop.
- Capture lifecycle CI: Xvfb + Openbox. These automated tests cover window tracking and capture, not overlay support on an X11 desktop.

| Desktop environment / window manager | Validation result |
| --- | --- |
| KDE Plasma (KWin), Hyprland, niri, Sway | Game tracking, capture, and overlay display confirmed. |
| MangoWM | Overlay display confirmed. Versions 0.15.x–0.16.1 are affected by the [mouse input issue](#mango-layer-shell-input-regression). |

These results apply to the environments tested. They do not guarantee compatibility with every version or configuration, or correct operation in windowed mode.

## Checking Your Environment

Run the following commands from the extracted directory:

```bash
uname -m
getconf GNU_LIBC_VERSION
printf 'session=%s WAYLAND_DISPLAY=%s DISPLAY=%s\n' \
  "${XDG_SESSION_TYPE:-unset}" "${WAYLAND_DISPLAY:-unset}" "${DISPLAY:-unset}"
ldd ./overmax
fc-match ':lang=ko' | head -n 1
```

- `uname -m`: `x86_64`
- `getconf GNU_LIBC_VERSION`: `glibc 2.35` or later
- Session: `session=wayland`, with both `WAYLAND_DISPLAY` and `DISPLAY` set
- `ldd`: no shared libraries reported as `not found`
- `fc-match`: a Korean-capable font is shown

## Installation and Launch

1. Download `overmax-linux-x86_64.tar.gz` from Releases.
2. Extract it into a directory where your user has write permission.
3. Run DJMAX RESPECT V through Proton/XWayland in **borderless fullscreen**. Windowed mode is usable, but effectively unsupported.
4. Run `./overmax` from a terminal in the same desktop session.

The directory containing the executable must be writable for automatic updates. When updating manually, preserve your data as described below.

## Settings, Records, and Backups

The data location depends on the runtime mode:

| Runtime mode | Data location |
| --- | --- |
| Portable (the default for a writable extracted bundle) | `settings.user.json` and `cache/` in the application directory. |
| Installed (when the application directory is not writable) | `settings.user.json` and `cache/` under `overmax/` in `XDG_DATA_HOME`, or under `.local/share/overmax/` relative to your home directory when `XDG_DATA_HOME` is unset. |

Close Overmax before backing up or restoring data. Preserve `settings.user.json` and the entire `cache/` directory, including the local record database `cache/record.db`.

For a manual update in Portable mode, extract the complete new bundle and copy the backed-up data into its application directory. In Installed mode, preserve the existing data directory; replacing the bundle does not require moving that data into it. Do not mix old executables or shared libraries with a new bundle.

## Launching Without a Terminal

In Overmax settings, select **System → Launch Linux App → Create Shortcut** to create an `overmax.desktop` entry in your user application menu. It uses the current executable and application directory. You can then launch Overmax from the application menu.

If you move the installation directory, the old shortcut will still point to the previous location. Run Overmax from its new location and create the shortcut again.

## Starting Overmax with the Game from Steam

Enter the following command under DJMAX RESPECT V **Properties → General → Launch Options** in Steam. Replace `OVERMAX_DIR` with the directory where you extracted Overmax.

```bash
sh -c '(cd "OVERMAX_DIR" && exec ./overmax) & exec "$@"' -- %command%
```

## Troubleshooting Startup Problems

Run `./overmax` from a terminal, check the first error shown, and apply the corresponding solution below.

| Symptom or check result | Solution |
| --- | --- |
| `Permission denied` | Run `chmod +x ./overmax`. |
| `Exec format error`, or `uname -m` is not `x86_64` | The current release bundle runs only on x86_64. |
| `GLIBC_2.35 not found`, or glibc is older than 2.35 | Run Overmax on a distribution with glibc 2.35 or later. |
| `ldd` reports `not found` | Install the distribution package that provides the reported shared library. |
| `WAYLAND_DISPLAY is not set` | Log out, sign in to a Wayland session, and launch Overmax from a terminal in that session. |
| `DISPLAY is not set` or `X11 connect failed` | Enable XWayland and run the game and Overmax in the same desktop session. |
| Overmax exits immediately without an error | Run `pgrep -a overmax` to check whether Overmax is already running. If it is not, try again from a normal desktop session with `XDG_RUNTIME_DIR` set. |
| `zwlr_layer_shell_v1 is unavailable` | Use a compositor that supports `wlr-layer-shell`. |
| Vulkan adapter, device, or surface error | Install or update the Vulkan driver and loader provided by your GPU vendor. |
| `Composite` or `MIT-SHM` error | Use a session with the XComposite and MIT-SHM XWayland extensions enabled. Gamescope sessions are not currently supported. |
| Korean text is missing, or `fc-match` fails | Install fontconfig and a Korean-capable font, then run `fc-cache -f`. |
| `DJMAX RESPECT V window not found` | Start the game first. Confirm that Proton uses XWayland rather than native Wayland and that the game and Overmax use the same `DISPLAY`. |
| The game window is found, but the overlay is not displayed correctly | Switch the game to borderless fullscreen. |
| The overlay does not follow the game after moving it in windowed mode | Reposition the overlay manually. |
| Permission error while saving settings or caches | Extract the bundle again into a directory where your user has write permission. |
| Overmax does not start after an update | Extract the entire new bundle and follow the [backup and restore instructions for your runtime mode](#settings-records-and-backups). Do not mix it with old executables or shared libraries. |

## Known Issues

### Recognition Failure with Fixed Window Sizes in Tiling Window Managers

Running the game at a fixed window size in a tiling window manager can change its aspect ratio and screen layout, causing recognition to fail even when window tracking and capture succeed.

Switch the game to **borderless fullscreen**. Aspect-ratio problems in windowed mode are outside the current support scope.

### Mango Layer-Shell Input Regression

On Mango 0.16.1, the Overmax layer-shell overlay may appear correctly while mouse input passes through to the game underneath. A similar regression affecting mouse and keyboard input on layer-shell surfaces after upgrading from 0.14.x to 0.15.x is reported in [Mango issue #1214](https://github.com/mangowm/mango/issues/1214).

Overmax's visible input region and `seat0` pointer binding were checked, but the compositor did not deliver `wl_pointer.enter`, `motion`, or `button` events. Changing Overmax settings or its input region did not resolve the observed issue.

Use an upstream release once a fix is confirmed, or another `wlr-layer-shell` compositor in the meantime. The upstream report identifies 0.14.x as unaffected in its tested configuration. Forcing `KeyboardInteractivity` is not used as a workaround because it can take keyboard focus away from the game.

## Currently Unsupported Features and Environments

- Guaranteed operation in windowed mode (usable, but effectively unsupported)
- Automatic overlay placement that follows the game window in windowed mode
- Windowed mode across multiple outputs
- GNOME (Mutter): the overlay requires `wlr-layer-shell`, which Mutter does not support. See the [protocol compatibility guide](https://github.com/wmww/gtk-layer-shell#supported-desktops).
- Gamescope and Steam Deck Gaming Mode: window tracking, capture, and overlay integration inside Gamescope are not currently supported or validated. Gamescope uses a [separate XWayland environment](https://github.com/ValveSoftware/gamescope#gamescope-the-micro-compositor-formerly-known-as-steamcompmgr), so validation on a regular desktop does not establish compatibility.
- X11 desktop sessions: the current overlay requires Wayland's `wlr-layer-shell`; no X11 overlay is provided
- Games running through native Wayland rather than XWayland
- Linux system tray icon

## Environments with Limited Validation

The following environments have not been tested enough to guarantee compatibility:

- Differences between compositors and distributions
- GPU vendor and driver combinations
- Exclusive fullscreen
- Fractional scaling and HiDPI combinations
- Different Proton versions
