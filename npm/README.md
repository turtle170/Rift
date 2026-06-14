# @turtle170/rift

> **Rift** — Your desktop pet code reviewer powered by local AI. 🦀 🦞

Rift is an interactive, local-first code analysis companion. It runs directly on your machine using `llama.cpp` to parse, analyze, and review your code repositories. It features a rich terminal user interface (TUI) with a customizable pet identity, live animations, and parallel code parsing.

---

## Features

- 🥚 **Interactive Hatching**: Run `rift hatch` to dynamically generate your companion pet's name, personality traits, and stats based on your machine GUID.
- 📂 **Multi-Language Tree-sitter Support**: Parses Rust, Python, JavaScript, TypeScript, C, C++, Go, and Java.
- ⚡ **Tokio/Rayon Parallel Analysis**: Spawns concurrent file workers (called Claws) to process your codebase's AST into streamlined Markdown S-expressions.
- 💻 **Automatic GPU Acceleration**: Automatically detects your system's hardware configuration (WMI / dedicated VRAM check) and fetches either the Vulkan-optimized or CPU-only build of `llama.cpp`.
- 🌶️ **Roast Mode**: Let your pet critique your code with absolutely zero empathy and maximum pedantry.
- ❓ **Grill Mode**: Interrogation mode where the pet asks you probing, difficult questions about your architecture choices instead of just listing bugs.

---

## Installation

You can install Rift globally using npm:

```bash
npm install -g @turtle170/rift
```

> **Note**: Rift is currently optimized for Windows systems (x64).

---

## Quick Start

### 1. Hatch Your Pet
To initialize Rift, download the local LLM model (~6.4 GB), and configure your pet:
```bash
rift hatch
```

### 2. Analyze a Project
Review your source code files under any directory:
```bash
rift analyze ./src
```

### 3. Roast Mode 
Instruct your pet to review your code with harsh, pedantic critiques:
```bash
rift roast ./src
```

### 4. Grill Mode
Instruct your pet to interrogate you and ask questions about your code layout:
```bash
rift grill ./src
```

---

## Options

- `--max-files <LIMIT>`: Set the maximum number of files to inspect during an analysis (defaults to `50` to prevent context overflow).

---

## License

This project is licensed under the Apache License 2.0.
