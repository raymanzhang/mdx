# MDictX - MDict Dictionary File Reader and Writer

[![Rust](https://img.shields.io/badge/Rust-2024-orange)]()
[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](https://www.gnu.org/licenses/agpl-3.0)
[![Version](https://img.shields.io/badge/Version-0.5.0-green)]()

libmdx is a comprehensive Rust library for reading, writing, and converting MDict dictionary files (.mdx, .mdd, .zdb formats). It provides high-performance parsing, full-text search capabilities, multiple compression/encryption methods, and support for various dictionary formats. 

## 📚 Table of Contents

- [Features](#features)
- [Supported Formats](#supported-formats)
- [Installation](#installation)
- [Contributing](#contributing)
- [License](#license)

## 🚀 Features

### Reading & Parsing
- ✅ **MDX Files**: Parse MDict dictionary files with HTML/Text/Binary content
- ✅ **MDD Files**: Extract resources (images, audio, fonts) from resource files
- ✅ **ZDB Files**: Read optimized MDict database format
- ✅ **Multiple Locales**: Support for various character encodings and locales
- ✅ **HTML Processing**: Automatic link rewriting and resource resolution

### Writing & Building
- ✅ **ZDB Creation**: Build optimized dictionary databases
- ✅ **Format Conversion**: Convert between MDX, MDD, and other formats
- ✅ **Custom Compression**: Choose from multiple compression algorithms
- ✅ **Encryption Support**: Secure dictionaries with various encryption methods

### Search & Indexing
- ✅ **Full-Text Search**: Build and query Tantivy-based search indexes
- ✅ **Fuzzy Matching**: Support for approximate keyword matching
- ✅ **Locale-Aware**: ICU collation for proper Unicode sorting

### Performance
- ✅ **Streaming I/O**: Efficient memory usage for large files
- ✅ **LRU Caching**: Smart caching for frequently accessed blocks
- ✅ **Parallel Processing**: Multi-threaded operations support
- ✅ **Optimized Indexing**: Fast binary search for key lookups

## 📦 Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
mdx = { version = "0.5.0", features = ["icu"] }
```

### Feature Flags

- **`icu` (default)**: Use ICU4X for Unicode collation (pure Rust, recommended)
- **`rust-icu`**: Use rust_icu for Unicode collation (requires system ICU library)

```toml
# Use rust_icu instead of icu
mdx = { version = "0.5.0", features = ["rust-icu"] }

# Both features can be enabled
mdx = { version = "0.5.0", features = ["icu", "rust-icu"] }
```

## 📜 License

MDictX is licensed under the GNU Affero General Public License v3.0 (AGPL-3.0).
See [LICENSE](LICENSE) for details.

## 👥 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## 📧 Contact

- Author: Rayman Zhang
- Email: raymanzhang@gmail.com
- Repository: https://github.com/raymanzhang/mdx

---

**Last Updated**: October 29, 2025
**Version**: 0.5.0
