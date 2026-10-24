# mpd-overlay

An AI-assisted custom MPD now-playing overlay for KDE Plasma on Wayland.
It displays track information in an overlay over fullscreen applications.
Based off of my old obs widget and some overlays I made for funsies

## Automatic installation

Install the latest GitHub release system-wide:

```sh
curl -fsSL https://raw.githubusercontent.com/xamionex/mpd-overlay/main/install.sh | sh
```

Install it only for your user (to `~/.local/bin`):

```sh
curl -fsSL https://raw.githubusercontent.com/xamionex/mpd-overlay/main/install.sh | sh -s -- --user
```

For help:

```sh
curl -fsSL https://raw.githubusercontent.com/xamionex/mpd-overlay/main/install.sh | sh -s -- --help
```

The system-wide install places `mpd-overlay` in `/usr/local/bin`. The user install places it in `~/.local/bin`; make sure that directory is on your `PATH`.

## Configuration

Configuration is read from `~/.config/mpd-overlay/config.toml`. On first run, a default file is created there.

Open the interactive configuration editor:

```sh
mpd-overlay -c
```

Enable or disable the user service:

```sh
mpd-overlay -e
mpd-overlay -d
```

## Dependencies

- GTK 3 (`gtk3`)
- MPD client support
- XWayland for the transparent click-through overlay

## Build from source

```sh
./devinstall.sh
```
