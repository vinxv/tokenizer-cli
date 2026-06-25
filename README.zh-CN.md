# Tokenizer Service (tokenizer-svc) &middot; 中文文档

高性能 Rust 微服务 / CLI，为 LLM 提供统一的 Tokenization 接口。支持 HuggingFace、ModelScope 和 OpenAI (tiktoken) 三种后端。

> 英文原版请见 [README.md](README.md)。

---

## 快速上手

### 1. 下载

从 [GitHub Releases](https://github.com/vinxv/tokenizer-cli/releases) 下载对应平台的预编译二进制。

```bash
# macOS / Linux 示例：下载并赋予执行权限
curl -fsSL -o tokenizer-cli https://github.com/vinxv/tokenizer-cli/releases/latest/download/tokenizer-cli-$(uname -s)-$(uname -m)
chmod +x tokenizer-cli
```

### 2. 拉取模型

```bash
# 从 HuggingFace 拉取（默认）
./tokenizer-cli pull Qwen/Qwen3-32B

# 从 ModelScope 拉取
./tokenizer-cli pull Qwen/Qwen2.5-0.5B-Instruct --source modelscope
```

### 3. 启动服务

```bash
./tokenizer-cli serve --enable-web
```

浏览器打开 `http://localhost:8080`，搞定。

> **从源码构建？** 见 [从源码构建](#从源码构建)。

---

## 功能特性

- **多后端支持**：集成 `fastokens`（BPE `tokenizer.json`）与 `tiktoken-rs`（OpenAI 计划中）。
- **多 Hub 模型拉取**：支持从 **HuggingFace Hub** 和 **ModelScope** 下载模型。
- **统一 CLI**：单个二进制文件，可启动服务、拉取模型、执行 Token 操作。
- **HTTP API**：标准化的 token 计数、编码、解码与模型发现接口。
- **Web 控制台**：可选的嵌入式 Web 调试界面。
- **懒加载与缓存**：模型按需加载，本地缓存复用。

---

## CLI 命令参考

`tokenizer-cli` 为主入口。任意命令加 `--json` 可输出机器可读的 JSON 格式。

| 命令 | 说明 |
| :--- | :--- |
| `serve` | 启动 HTTP API 服务，加 `--enable-web` 开启控制台 |
| `pull <model>` | 下载 tokenizer 文件到本地缓存 |
| `models` | 列出当前注册表中所有 tokenizer |
| `count <model> <text>` | 统计指定文本的 token 数 |
| `encode <model> <text>` | 将文本编码为 token ID |
| `decode <model> <ids...>` | 将 token ID 解码为文本 |
| `--help` | 打印完整帮助信息 |
| `ping` | 服务健康检查 |

---

## HTTP API

### GET `/v1/models`
返回缓存或已加载的全部模型列表。
每个模型包含 `has_chat_template`、`chat_template_source` 和 `chat_template_name` 字段，Web 控制台据此决定是否显示 Render 选项卡。

### POST `/v1/token/count`
**请求体**：`{"model": "gpt-4o", "text": "Hello world"}`
**响应**：`{"model": "gpt-4o", "count": 2}`

### POST `/v1/token/encode`
**请求体**：`{"model": "Qwen/Qwen3-32B", "text": "Hello world"}`
**响应**：`{"model": "Qwen/Qwen3-32B", "ids": [9707, 1879], "tokens": ["Hello", " world"]}`

### POST `/v1/token/decode`
**请求体**：`{"model": "Qwen/Qwen3-32B", "ids": [9707, 1879]}`
**响应**：`{"model": "Qwen/Qwen3-32B", "text": "Hello world"}`

### POST `/v1/chat/render`
将 Chat Completions 格式的 `messages` 渲染为原始 prompt 文本，并返回其 token 数量。
请求体中的额外字段会作为 Jinja 模板变量传入上下文。

模板解析优先级：
1. 从缓存 `tokenizer_config.json` 读取 `chat_template`。
2. 否则使用缓存的 `.jinja` 模板文件（如 `chat_template.jinja`）。
3. 若都不存在，返回 `INVALID_INPUT` 错误。

**请求体**：`{"model": "Qwen/Qwen3-32B", "messages": [{"role": "user", "content": "你好"}], "add_generation_prompt": true}`
**响应**：`{"model": "Qwen/Qwen3-32B", "text": "...", "count": 42, "template_source": "jinja_file", "template_name": "chat_template.jinja"}`

---

## 配置详解

服务通过 `config.yaml`、环境变量和 CLI 参数组合配置。优先级：**CLI 参数 > 环境变量 > config.yaml > 默认值**。

### config.yaml

在项目根目录创建 `config.yaml`（可参考 `config.example.yaml`）：

| 字段 | 类型 | 默认值 | 说明 |
| :--- | :--- | :--- | :--- |
| `host` | string | `0.0.0.0` | HTTP 服务器绑定地址 |
| `port` | u16 | `8080` | HTTP 服务器监听端口 |
| `cache_dir` | string | `~/.cache/tokenizer-svc` | 模型文件本地缓存目录 |
| `model_source` | string | `huggingface` | 模型下载 Hub：`huggingface` 或 `modelscope` |
| `hf_token` | string? | — | HuggingFace API Token（访问私有/受限模型） |
| `ms_token` | string? | — | ModelScope SDK Token（访问私有/受限模型） |
| `allow_online_download` | bool | `true` | 是否允许请求时自动下载模型；设为 `false` 则必须提前缓存 |

### 环境变量

所有配置项均可通过 `TOKSVC_` 前缀的环境变量覆盖：

| 环境变量 | 对应字段 |
| :--- | :--- |
| `TOKSVC_HOST` | `host` |
| `TOKSVC_PORT` | `port` |
| `TOKSVC_CACHE_DIR` | `cache_dir` |
| `TOKSVC_MODEL_SOURCE` | `model_source` |
| `TOKSVC_HF_TOKEN` | `hf_token` |
| `TOKSVC_MS_TOKEN` | `ms_token` |
| `TOKSVC_ALLOW_ONLINE_DOWNLOAD` | `allow_online_download` |

### CLI 参数

| 参数 | 适用命令 | 说明 |
| :--- | :--- | :--- |
| `--json` | 全局 | 以 JSON 格式输出（与 API 响应结构一致） |
| `--log-level` | 全局 | 日志级别：`error` / `warn` / `info` / `debug` / `trace`（默认 `info`） |
| `--enable-web` | `serve` | 开启嵌入式 Web 控制台 |
| `--source` | `pull` | 模型 Hub：`huggingface` 或 `modelscope` |
| `--token` | `pull` | HuggingFace API Token（覆盖 env/config） |
| `--ms-token` | `pull` | ModelScope SDK Token（覆盖 env/config） |

### 配置示例

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
# 环境变量覆盖端口 9090 → 3000
TOKSVC_PORT=3000 ./tokenizer-cli serve --enable-web
```

---

## 从源码构建

需要 [Rust](https://rustup.rs) 环境：

```bash
git clone https://github.com/vinxv/tokenizer-cli.git
cd tokenizer-cli
cargo build --release
```

二进制文件位于 `target/release/tokenizer-cli`。
