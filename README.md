# Tokenizer Service (tokenizer-svc)

A high-performance, Rust-based microservice and CLI for LLM tokenization. It provides a unified interface for HuggingFace, ModelScope, and OpenAI (tiktoken) tokenizers.

## Features

- **Multi-Backend Support**: Integration with `tokenizers` (HuggingFace) and `tiktoken-rs` (OpenAI).
- **Multi-Hub Model Pulling**: Seamlessly download models from **HuggingFace Hub** and **ModelScope**.
- **Unified CLI**: A single binary for serving the API, pulling models, and performing token operations.
- **REST API**: Standardized endpoints for token counting, encoding, decoding, and model discovery.
- **Web Dashboard**: An optional, built-in internal web interface for interactive debugging and testing.
- **Prometheus Metrics**: Ready for production monitoring with built-in metrics.
- **Lazy Loading & Caching**: Models are loaded on-demand and cached locally for efficiency.

---

## Quick Start

### 1. Installation

Ensure you have Rust installed. Clone the repository and build:

```bash
cargo build --release
```

The binary will be available at `target/release/tokenizer-cli`.

### 2. Pull a Model

Download a tokenizer from HuggingFace (default) or ModelScope:

```bash
# Pull from HuggingFace
./target/release/tokenizer-cli pull bert-base-uncased

# Pull from ModelScope
./target/release/tokenizer-cli pull iic/nlp_corom_sentence-embedding_chinese-base --source modelscope
```

### 3. Start the Service

```bash
./target/release/tokenizer-cli serve --enable-web
```

The service will start on `http://0.0.0.0:3000`. You can access the web dashboard at `http://localhost:3000`.

---

## CLI Reference

`tokenizer-cli` is the main entry point. Use `--json` with any command for machine-readable output.

| Command | Description |
| :--- | :--- |
| `serve` | Start the HTTP API server. Add `--enable-web` for the dashboard. |
| `pull <model>` | Download tokenizer files to the local cache. |
| `models` | List all tokenizers currently available in the registry. |
| `count <model> <text>` | Count tokens in the provided text. |
| `encode <model> <text>` | Encode text into token IDs. |
| `decode <model> <ids...>`| Decode token IDs back into text. |
| `ping` | Health check for the service. |

---

## API Reference

### GET `/v1/models`
Returns a list of all models discovered in the cache or loaded in memory.

### POST `/v1/token/count`
**Body**: `{"model": "gpt-4o", "text": "Hello world"}`
**Response**: `{"model": "gpt-4o", "count": 2}`

### POST `/v1/token/encode`
**Body**: `{"model": "bert-base-uncased", "text": "Hello world"}`
**Response**: `{"model": "bert-base-uncased", "ids": [7592, 2088]}`

### POST `/v1/token/decode`
**Body**: `{"model": "bert-base-uncased", "ids": [7592, 2088]}`
**Response**: `{"model": "bert-base-uncased", "text": "hello world"}`

### GET `/metrics`
Exposes Prometheus-formatted metrics.

---

## Configuration

The service can be configured via `config.yaml`, environment variables, or CLI flags. See `config.example.yaml` for a full reference.

Key environment variables:
- `TOKSVC_HOST`: Interface to bind to (default: `0.0.0.0`).
- `TOKSVC_PORT`: Port to listen on (default: `3000`).
- `TOKSVC_CACHE_DIR`: Directory for model storage (default: `~/.cache/huggingface`).
- `HF_TOKEN`: HuggingFace API token for private models.
- `TOKSVC_MS_TOKEN`: ModelScope SDK token.

---

## Development

### Requirements
- Rust 1.75+
- `libssl` (for `reqwest` and `hf-hub`)

### Running Tests
```bash
cargo test
```

### Building for Multiple Platforms
The project uses GitHub Actions to build binaries for Linux (glibc/musl) and macOS across x86_64 and arm64 architectures. Check `.github/workflows/build.yml` for details.
