# Tabex

A cli tool for extraction tabular data from a URL.

## Usage
Please ensure you have `cargo` and the rust toolchain (`rustup`) installed.

```bash

# Extract all tables from a website and output as JSON
cargo run -- -u https://example.com/page-with-tables

# Save results to a file
cargo run -- -u https://example.com/data-page -o results.json

# Output in CSV format
cargo run -- -u https://example.com/data-page -f csv -o tables.csv

# For production build run

cargo build --release
```
