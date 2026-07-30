# typesass

![typesass logo](src/assets/typesass-logo.png)

typesass is a lightweight desktop speech-to-text assistant for dictation,
translation, quick Q&A, and automatic paste workflows. It is built with Tauri,
Vite, and TypeScript, and currently uses Xiaomi Mimo-compatible OpenAI-style
API endpoints for ASR and text generation.

## Features

- Start recording from a global shortcut.
- Transcribe speech, then optionally polish the dictated text with AI.
- Translate spoken content and paste the result into the active input field.
- Ask quick voice questions and view the answer in the local hub.
- Keep local history, dictionary entries, settings, and tray-menu workflows.
- Store API credentials outside the source code.

## Default Shortcuts

| Action | Shortcut |
| --- | --- |
| Dictation | `Control + P` |
| Translation | `Control + T` |
| Ask | `Control + Space` |

## Requirements

- Node.js 20 or later
- npm
- Rust and Cargo for the Tauri desktop app
- A Xiaomi Mimo API key

## Web Preview

Use the web preview when Rust/Cargo is not installed yet:

```bash
npm install
npm run build
npm run preview:web
```

Open the URL printed in the terminal, enter your Mimo API key, and start
recording.

You can also provide the key through an environment variable:

```bash
MIMO_API_KEY=your_api_key npm run preview:web
```

## Desktop Development

Install Rust first, then run the Tauri app:

```bash
npm install
npm run dev
```

You can also provide the key through an environment variable:

```bash
MIMO_API_KEY=your_api_key npm run dev
```

Build the desktop app:

```bash
npm run tauri:build
```

## Default Model Configuration

| Setting | Value |
| --- | --- |
| Base URL | `https://token-plan-cn.xiaomimimo.com/v1` |
| ASR model | `mimo-v2.5-asr` |
| AI model | `mimo-v2.5` |
| Language | Auto detect |

## Privacy And Security

typesass does not hard-code API keys and does not store them in `localStorage`.
On macOS, keys entered in the desktop settings page are stored in Keychain. You
can also provide `MIMO_API_KEY` at runtime if you prefer environment-based
configuration.

## Repository Mirrors

This project is mirrored as:

- GitHub: `typesass`
- Alibaba Cloud Codeup: `aiTool`

The local `origin` remote is configured to keep the Codeup repository as the
fetch source and push to both Codeup and GitHub.

## License

MIT
