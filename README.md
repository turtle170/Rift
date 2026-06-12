# 🦞 Rift

> Your desktop pet code reviewer — powered by local AI and Tree-sitter.

Rift hatches a unique crustacean companion derived from your machine's identity, then uses a local Gemma 4 model to review your code with personality.

## Installation

```powershell
npm install -g rift
```

> **Windows 11 x64 only.** Requires Node.js 18+.

## Quick Start

```powershell
# Hatch your Rift pet (downloads ~6.4 GB model on first run)
rift hatch

# Analyze a directory or file
rift analyze ./src
rift analyze ./my_project --max-files 100
```

## How it Works

1. **`rift hatch`** reads your Windows Machine GUID and derives a unique name (e.g. *Cyber Clawy Rifty*) plus personality stats for your pet. It then downloads [llama.cpp](https://github.com/ggml-org/llama.cpp) and [Gemma 4 E4B Q6_K](https://huggingface.co/unsloth/gemma-4-E4B-it-GGUF).

2. **`rift analyze <path>`** walks your code, builds a Tree-sitter AST, simplifies it to a Markdown summary, and feeds it to your pet for a personality-flavoured review — displayed live in a TUI.

## Pet Stats

Each pet has 6 stats (0–255) derived from your machine GUID:

| Stat | Effect |
|------|--------|
| `debuggability` | How precisely it spots bugs |
| `curiosity` | How many questions it asks |
| `unpredictability` | How chaotic/tangential its commentary is |
| `chattiness` | How verbose the review is |
| `pedantry` | How nitpicky about style |
| `empathy` | How encouraging vs. harsh |

## Supported Languages

Rust · Python · JavaScript · TypeScript · C · C++ · Go · Java

## Storage

| Asset | Location |
|-------|----------|
| Pet config | `%APPDATA%\rift\pet.toml` |
| llama.cpp binary | `D:\rift\llama\` |
| Gemma 4 model | `D:\rift\models\` |

The model download is **resumable** — safe to interrupt and retry.

## GPU Detection

`rift hatch` auto-detects your GPU:
- **Discrete GPU** (NVIDIA, AMD RX, Intel Arc) → Vulkan-accelerated llama.cpp
- **Integrated GPU** (Intel UHD, AMD APU) → CPU llama.cpp

## License

Apache 2.0 — see [LICENSE](LICENSE).
