# Tokenizer Service (tokenizer-svc)

A high-performance, Rust-based microservice and CLI for LLM tokenization. It provides a unified interface for HuggingFace, ModelScope, and OpenAI (tiktoken) tokenizers.

---

## Get Started

### 1. Download

Grab the latest binary for your platform from [GitHub Releases](https://github.com/vinxv/tokenizer-cli/releases).

```bash
# Example: download and make executable (macOS / Linux)
curl -L -o tokenizer-cli https://github.com/vinxv/tokenizer-cli/releases/latest/download/tokenizer-cli-$(uname -s)-$(uname -m)
chmod +x tokenizer-cli
```

### 2. Pull a Model

```bash
# From HuggingFace (default)
./tokenizer-cli pull Qwen/Qwen3-32B

# From ModelScope
./tokenizer-cli pull Qwen/Qwen2.5-0.5B-Instruct --source modelscope
```

### 3. Start the Service

```bash
./tokenizer-cli serve --enable-web
```

Open `http://localhost:8080` in your browser — you're done.

> **Building from source?** See [Building from Source](#building-from-source).

---

## Features

- **Multi-Backend Support**: Integration with `fastokens` for BPE `tokenizer.json` files and `tiktoken-rs` for planned OpenAI support.
- **Multi-Hub Model Pulling**: Seamlessly download models from **HuggingFace Hub** and **ModelScope**.
- **Unified CLI**: A single binary for serving the API, pulling models, and performing token operations.
- **REST API**: Standardized endpoints for token counting, encoding, decoding, and model discovery.
- **Web Dashboard**: An optional, built-in internal web interface for interactive debugging and testing.
- **Lazy Loading & Caching**: Models are loaded on-demand and cached locally for efficiency.

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
| `decode <model> <ids...>` | Decode token IDs back into text. |
| `ping` | Health check for the service. |

---

## API Reference

### GET `/v1/models`
Returns a list of all models discovered in the cache or loaded in memory.
Each model includes `has_chat_template`, `chat_template_source`, and `chat_template_name`; the Web Dashboard uses them to show the Render tab only for models with a cached `chat_template` or `.jinja` template.

### POST `/v1/token/count`
**Body**: `{"model": "gpt-4o", "text": "Hello world"}`
**Response**: `{"model": "gpt-4o", "count": 2}`

### POST `/v1/token/encode`
**Body**: `{"model": "Qwen/Qwen3-32B", "text": "Hello world"}`
**Response**: `{"model": "Qwen/Qwen3-32B", "ids": [9707, 1879], "tokens": ["Hello", " world"]}`

### POST `/v1/token/decode`
**Body**: `{"model": "Qwen/Qwen3-32B", "ids": [9707, 1879]}`
**Response**: `{"model": "Qwen/Qwen3-32B", "text": "Hello world"}`

### POST `/v1/chat/render`
Renders Chat Completions-style `messages` to raw prompt text and returns its token count.
Extra JSON fields in the request body are passed through as Jinja template variables.

Template resolution:
1. `chat_template` is read from cached `tokenizer_config.json`.
2. Otherwise, a cached `.jinja` template file such as `chat_template.jinja` is used.
3. If none exists, the request returns `INVALID_INPUT`.

**Body**: `{"model": "Qwen/Qwen3-32B", "messages": [{"role": "user", "content": "Hello"}], "add_generation_prompt": true}`
**Response**: `{"model": "Qwen/Qwen3-32B", "text": "...", "count": 42, "template_source": "jinja_file", "template_name": "chat_template.jinja"}`

---

## Configuration

Configuration is loaded from three layers in order of precedence:
**CLI flags > environment variables > `config.yaml` > defaults**.

### config.yaml

Create a `config.yaml` in the project root (see `config.example.yaml` for a template):

| Field | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `host` | string | `0.0.0.0` | Interface to bind the HTTP server on |
| `port` | u16 | `8080` | Port to listen on |
| `cache_dir` | string | `~/.cache/tokenizer-svc` | Local cache directory for downloaded model files |
| `model_source` | string | `huggingface` | Model hub: `huggingface` or `modelscope` |
| `hf_token` | string? | — | HuggingFace API token (for gated / private models) |
| `ms_token` | string? | — | ModelScope SDK token (for gated / private models) |
| `allow_online_download` | bool | `true` | If `false`, models must be pre-cached; on-demand download is disabled |

### Environment Variables

All config fields can be overridden with `TOKSVC_`-prefixed environment variables:

| Variable | Maps to |
| :--- | :--- |
| `TOKSVC_HOST` | `host` |
| `TOKSVC_PORT` | `port` |
| `TOKSVC_CACHE_DIR` | `cache_dir` |
| `TOKSVC_MODEL_SOURCE` | `model_source` |
| `TOKSVC_HF_TOKEN` | `hf_token` |
| `TOKSVC_MS_TOKEN` | `ms_token` |
| `TOKSVC_ALLOW_ONLINE_DOWNLOAD` | `allow_online_download` |

### CLI Flags

| Flag | Command | Description |
| :--- | :--- | :--- |
| `--json` | global | Emit JSON output matching the HTTP API response shape |
| `--log-level` | global | Log level: `error` / `warn` / `info` / `debug` / `trace` (default: `info`) |
| `--enable-web` | `serve` | Enable the embedded Web Dashboard on `/` |
| `--source` | `pull` | Model hub: `huggingface` or `modelscope` |
| `--token` | `pull` | HuggingFace API token (overrides env / config) |
| `--ms-token` | `pull` | ModelScope SDK token (overrides env / config) |

### Example

```yaml
# config.yaml
host: "127.0.0.1"
port: 9090
cache_dir: "/data/tokenizer-cache"
model_source: "modelscope"
hf_token: "hf_your_token_here"
allow_online_download: false
```

```bash
# env overrides port 9090 → 3000
TOKSVC_PORT=3000 ./tokenizer-cli serve --enable-web
```

---

## Building from Source

Requires [Rust](https://rustup.rs). Clone the repository and build:

```bash
git clone https://github.com/vinxv/tokenizer-cli.git
cd tokenizer-cli
cargo build --release
```

The binary will be at `target/release/tokenizer-cli`.

---

## Documentation

- [中文文档](README.zh-CN.md)
