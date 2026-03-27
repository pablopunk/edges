# Edges

A lightweight window border tool for macOS, written in Rust. Inspired by [JankyBorders](https://github.com/FelixKratz/JankyBorders).

## Install

```bash
brew install --HEAD pablopunk/brew/edges
```

## Usage

```bash
# Start as a background service (runs at login)
brew services start edges

# Or run directly
edges

# Custom width and colors (ARGB hex)
edges --width 6.0 --active-color 0xffe2e2e3 --inactive-color 0xff414550

# Square corners
edges --style square

# HiDPI mode (2x resolution)
edges --hidpi
```

### Options

| Flag | Description | Default |
|------|-------------|---------|
| `--width <N>` | Border width in points | `4.0` |
| `--style <S>` | `round`, `square`, or `uniform` | `round` |
| `--active-color <HEX>` | Focused window border (ARGB) | `0xffe1e3e4` |
| `--inactive-color <HEX>` | Unfocused window border (ARGB) | `0x00000000` |
| `--hidpi` | Enable HiDPI rendering | off |

### Service management

```bash
brew services start edges   # start and enable at login
brew services stop edges    # stop
brew services restart edges # restart
```

## Requirements

- macOS 14.0+
- Accessibility permissions (System Settings → Privacy & Security)
- Screen Recording permissions

## Building from source

```bash
cargo build --release
./target/release/edges
```

## License

MIT

## Acknowledgments

- [JankyBorders](https://github.com/FelixKratz/JankyBorders) by FelixKratz
