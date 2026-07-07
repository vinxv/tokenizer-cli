# Tokenizer Service (tokenizer-svc)

> **One self-contained binary** that exposes an LLM tokenizer over HTTP, from the CLI, and from a built-in web dashboard — pulling models from **HuggingFace** and **ModelScope**.

[![Build](https://github.com/vinxv/tokenizer-cli/actions/workflows/build.yml/badge.svg)](https://github.com/vinxv/tokenizer-cli/actions/workflows/build.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](#license)
[![Rust](https://img.shields.io/badge/rust-2021%2B-orange.svg)](https://www.rust-lang.org/)
[![Release](https://img.shields.io/github/v/release/vinxv/tokenizer-cli)](https://github.com/vinxv/tokenizer-cli/releases)

[中文文档](README.zh-CN.md) · [Quick start](#get-started) · [HTTP API](#http-api) · [CLI](#cli-reference)

<p align="center">
  <img src="assets/dashboard.png" alt="Tokenizer Web Dashboard" width="820">
</p>

---

## Why

Counting tokens, pre-splitting text, and rendering chat templates are everyday tasks when building on LLMs — for cost estimation, rate limiting, context-budgeting, and RAG chunking. The usual tools each leave a gap:

- **`transformers`** pulls in a Python runtime and gigabytes of dependencies just to count tokens.
- **`tiktoken`** only covers OpenAI models.

`tokenizer-svc` is one static binary (~12 MB stripped) that gives you an HTTP API, a CLI, and a built-in web dashboard for interactive debugging，supporting HuggingFace and ModelScope, with chat-template rendering included. The dashboard is fully self-contained: fonts are embedded in the binary, so it makes no requests to any external CDN.

---

## Get Started

### 1. Download

Grab the latest binary for your platform from [GitHub Releases](https://github.com/vinxv/tokenizer-cli/releases).

```bash
# Example: download and make executable (macOS / Linux)
curl -fsSL -o tokenizer-cli https://github.com/vinxv/tokenizer-cli/releases/latest/download/tokenizer-cli-$(uname -s)-$(uname -m)
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

### 4. Or use the CLI directly

No server required — `count`, `encode`, and `decode` work straight on the local cache:

```bash
./tokenizer-cli count  Qwen/Qwen3-32B "Hello world"
./tokenizer-cli encode Qwen/Qwen3-32B "Hello world"
./tokenizer-cli decode Qwen/Qwen3-32B 9707 1879
```

Add `--json` to any command for output matching the HTTP API response shape.

> **Building from source?** See [Building from Source](#building-from-source).

---

## Web Dashboard

Run `serve --enable-web` and open `http://localhost:8080` for an interactive dashboard with four tabs:

- **Encode** — turn text into token IDs and a per-token table (Index / ID / text), with virtualized scrolling for long inputs.
- **Decode** — paste token IDs (comma, space, or JSON-array separated) and get the text back.
- **Count** — instant token count for any input.
- **Render** — render Chat Completions `messages` to raw prompt text via the model's chat template, and count the result. Shown only for models that ship a `chat_template`.

Every result shows request latency and a one-click copy button.

---

## Features

- **Multi-Hub Model Pulling**: download tokenizer files from **HuggingFace Hub** and **ModelScope**, cached locally and loaded on demand.
- **Unified CLI**: a single binary for serving the API, pulling models, and performing token operations.
- **HTTP API**: standardized endpoints for token counting, encoding, decoding, model discovery, and chat-template rendering.
- **Web Dashboard**: an optional, built-in internal web interface for interactive debugging and testing.
- **Lazy Loading & Caching**: models are loaded on-demand and cached locally for efficiency.

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
| `--help` | Print full help information. |

---

## HTTP API

### GET `/v1/models`
Returns a list of all models discovered in the cache or loaded in memory.
Each model includes `has_chat_template`, `chat_template_source`, and `chat_template_name`; the Web Dashboard uses them to show the Render tab only for models with a cached `chat_template` or `.jinja` template.

### POST `/v1/token/count`
**Body**: `{"model": "Qwen/Qwen3-32B", "text": "Hello world"}`
**Response**: `{"model": "Qwen/Qwen3-32B", "count": 2}`

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

## License

Licensed under the [MIT License](LICENSE).
