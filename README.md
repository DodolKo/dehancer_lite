# Dehancer Lite

[![Version](https://img.shields.io/badge/version-0.1.0-blue)](https://github.com/DodolKo/dehancer_lite)
[![Rust](https://img.shields.io/badge/rust-2021-orange)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-green)](LICENSE)

A lightweight film stack simulator inspired by Dehancer, built with Rust and egui. This project provides a minimal viable product (MVP) for applying film-like effects to images, including bloom, grain, halation, and more. It supports both native desktop applications and web-based versions using WebGPU.

## Features

- **Film Stack Effects**: Simulate various film processing stages including:
  - Bloom and halation effects
  - Film compression
  - Grain simulation
  - Color grading and finishing
- **Real-time Processing**: Tune presets and effect modules in real-time for interactive experimentation.
- **Cross-Platform**: Native desktop app using egui/eframe, and web version with WebGPU support.
- **ACEScg Workflow**: Internally processes in ACEScg color space for physically credible results.
- **Image Support**: Load and process various image formats (PNG, JPEG, WebP, BMP, TIFF).

## Screenshots

*(Add screenshots here if available)*

## Installation

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (version 1.70 or later)
- For web version: [Trunk](https://trunkrs.dev/) (install with `cargo install trunk`)
- For native version: Ensure you have a compatible graphics backend (e.g., Vulkan, Metal, or DirectX)

### Clone the Repository

```bash
git clone https://github.com/DodolKo/dehancer_lite.git
cd dehancer_lite
```

## Building

### Native Build

To build the native desktop application:

```bash
cargo build --release
```

The binary will be located in `target/release/dehancer_lite`.

### Web Build

To build the web version:

```bash
trunk build --release
```

The built files will be in the `dist/` directory.

## Running

### Native Application

Run the native version:

```bash
cargo run
```

This will launch the egui-based desktop application with a window size of 1440x900.

### Web Application

To run the web version locally:

```bash
trunk serve
```

Open your browser to `http://localhost:8080` (or the port specified by Trunk).

You can drop or select an image file, then adjust presets and effects in real-time.

## Usage

1. **Load an Image**: Use the file picker or drag-and-drop to load an image.
2. **Adjust Effects**: Use the UI controls to tune various film stack parameters.
3. **Export**: Save the processed image (implementation details in the code).

## Project Structure

- `src/`: Main Rust source code
  - `app/`: Application logic and UI
  - `color/`: Color space utilities
  - `effects/`: Individual effect modules (bloom, grain, etc.)
  - `gpu/`: GPU-related code (WGSL shaders)
  - `halation/`: Halation effect implementation
  - `pipeline/`: Processing pipeline
- `web/`: Web-specific assets (CSS, JS)
- `tests/`: Unit and integration tests
- `Trunk.toml`: Configuration for Trunk (web build tool)
- `Cargo.toml`: Rust project configuration

## Development

### Running Tests

```bash
cargo test
```

### Code Formatting

```bash
cargo fmt
```

### Linting

```bash
cargo clippy
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- Inspired by [Dehancer](https://dehancer.com/) film processing software
- Built using [egui](https://github.com/emilk/egui) for the UI
- WebGPU support via [eframe](https://github.com/emilk/egui/tree/master/eframe)
- ACEScg color space implementation based on public references

## Contact

For questions or feedback, please open an issue on GitHub.</content>
<parameter name="filePath">/home/dodolko/dev/dehancer_lite/README.md