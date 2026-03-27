# Edges

A lightweight window border tool for macOS, written in Rust. Inspired by [JankyBorders](https://github.com/FelixKratz/JankyBorders).

## Building

```bash
cargo build --release
```

## Usage

```bash
# Default (JankyBorders-style defaults)
./target/release/edges

# Custom width and colors (ARGB hex)
./target/release/edges --width 6.0 --active-color 0xffe2e2e3 --inactive-color 0xff414550

# Square corners
./target/release/edges --style square

# HiDPI mode (2x resolution)
./target/release/edges --hidpi
```

### Options

| Flag | Description | Default |
|------|-------------|---------|
| `--width <N>` | Border width in points | `4.0` |
| `--style <S>` | `round`, `square`, or `uniform` | `round` |
| `--active-color <HEX>` | Focused window border (ARGB) | `0xffe1e3e4` |
| `--inactive-color <HEX>` | Unfocused window border (ARGB) | `0x00000000` |
| `--hidpi` | Enable HiDPI rendering | off |

## Requirements

- macOS 14.0+
- Accessibility permissions (System Settings → Privacy & Security)
- Screen Recording permissions

## License

MIT

## Acknowledgments

- [JankyBorders](https://github.com/FelixKratz/JankyBorders) by FelixKratz
