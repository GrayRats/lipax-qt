# LipaX

[Русская версия](README.ru.md)

**Game text translation for Linux — in a separate window or over the original text.**

LipaX captures the selected game window, recognizes the text and shows the translation. It targets **KDE Plasma 6 / KWin / Wayland**. The interface is system Qt 6.12+ with QML; frame processing is written in Rust.

<div align="center">

<a href="#features"><img src="https://img.shields.io/badge/FEATURES-2F81F7?style=for-the-badge" height="36" alt="FEATURES"></a>
<a href="#details"><img src="https://img.shields.io/badge/DETAILS-6E40C9?style=for-the-badge" height="36" alt="DETAILS"></a>
<a href="#quick-start"><img src="https://img.shields.io/badge/QUICK--START-1A7F37?style=for-the-badge" height="36" alt="QUICK START"></a>
<a href="#installation"><img src="https://img.shields.io/badge/INSTALLATION-0969DA?style=for-the-badge" height="36" alt="INSTALLATION"></a>
<a href="#diagnostics-and-limitations"><img src="https://img.shields.io/badge/DIAGNOSTICS-BF8700?style=for-the-badge" height="36" alt="DIAGNOSTICS"></a>
<a href="#documentation"><img src="https://img.shields.io/badge/DOCUMENTATION-8250DF?style=for-the-badge" height="36" alt="DOCUMENTATION"></a>
<a href="#authors"><img src="https://img.shields.io/badge/AUTHOR_AND_PROJECTS-57606A?style=for-the-badge" height="36" alt="AUTHOR AND PROJECTS"></a>
<a href="README.ru.md"><img src="https://img.shields.io/badge/РУССКИЙ-24292F?style=for-the-badge" height="36" alt="РУССКИЙ"></a>

</div>

![LipaX main window](docs/screenshots/main.png)

<details>
<summary>Translation appearance settings</summary>

![LipaX settings — translation appearance](docs/screenshots/settings.png)

</details>

<a id="features"></a>
## Features

- **Translation during play.** Continuous text tracking or a one-off translation on demand. Repeated phrases come from the cache.
- **Up to three capture regions.** Subtitles, dialogue and menus are read separately. Each region can be switched off and given its own languages and OCR engine.
- **Two display modes.** A separate translation window over the game, or an overlay on the original text with fitted size and line breaks.
- **Several OCR engines.** Tesseract, RapidOCR (PP-OCRv5), MeikiOCR (Japanese), PaddleOCR, and an Auto mode that reads again when confidence is low.
- **Local and online translation.** Bergamot, Google Translate, Yandex Translate, DeepL, Microsoft Translator and a custom API.
- **Font matching for the original.** In overlay mode the serifs, letter width, monospacing, weight and slant are analysed. The closest family is chosen from 36 bundled fonts, including the kizurium set. PT Serif has a real italic. This finds a similar style, not the exact font name.
- **Appearance.** Font, text colour, background, border, opacity, corner radius, shadow and outline. The translation window can be pinned, moved and scrolled when the text is long.
- **Game profiles.** Regions and processing settings are remembered per game when capturing through KWin.
- **Control without switching windows.** Configurable hotkeys and a system tray. With minimize-to-tray enabled, translation keeps running after the main window is closed.
- **History and OCR checks.** A history of originals and translations with copy, a view of the recognized text, manual image filters and automatic frame processing.
- **Diagnostics.** The Status tab shows whether the engines, models and capture components are ready. The log helps with errors.

<a id="details"></a>
## Details

### Bergamot: offline translation

Bergamot translates on your computer without sending the text to an online service. **The native engine is part of the LipaX Arch package**, so the "Bergamot: CLI path" field can be left empty.

When you choose a language pair, LipaX looks for a model in this order: the saved path, its own cache, and the model directories of Firefox profiles. Files found in Firefox are copied into the LipaX cache. If there is no suitable model, the application downloads it from Mozilla's official CDN and saves the path in the settings.

- Every file is checked by size and SHA-256. An unfinished download is never treated as an installed model.
- Searching and downloading run in the background. Progress is shown in the settings and in the tray tooltip, and minimizing the application does not interrupt the download.
- You can point to your own installation with **Set model path manually**. A manual set must also match the verified models in the catalogue.
- Once a model is installed, translation works without a network connection. The first download needs the internet. Only the language directions in the catalogue are available; there is no automatic pivoting through another language.

Default cache: `~/.local/share/LipaX/bergamot-models/<pair>/` (respects `XDG_DATA_HOME`). More: [Bergamot setup](docs/Bergamot.md).

### Recognition: engines

| Engine | Requirements | Notes |
|---|---|---|
| **Tesseract** | `tesseract-data-*` language data | The default engine. Several languages at once. English data is a dependency of the Arch package. |
| **RapidOCR (PP-OCRv5)** | ONNX Runtime and a model downloaded in Settings | Runs inside the application, no Python. The model follows the primary language. Thread count and GPU can be set if the installed library was built with GPU support. |
| **MeikiOCR (Japanese)** | ONNX Runtime and a model downloaded in Settings (about 46 MB, LGPL-3.0) | Runs inside the application, no Python. Reads Japanese game text, including vertical text. |
| **PaddleOCR** | A Python environment with PaddleOCR | Optional. Reads the primary language. Setup is described in a separate guide. |
| **Auto** | Tesseract and the engines you enabled | When Tesseract is unsure, it tries MeikiOCR (Japanese, if its model is installed), then RapidOCR, and then PaddleOCR if RapidOCR is unavailable. |

**RapidOCR and MeikiOCR models are downloaded only with the "Download" button** in the Recognition tab. When you change the language, the application offers to download the missing model, but it never downloads anything by itself. Sizes and SHA-256 are checked before installation. Recognition itself needs no network.

Model sources: [RapidAI/RapidOCR](https://github.com/RapidAI/RapidOCR) and [rtr46/meikiocr](https://github.com/rtr46/meikiocr). Guides: [RapidOCR](docs/RapidOCR.md), [MeikiOCR](docs/MeikiOCR.md), [PaddleOCR](docs/PaddleOCR.md) (in Russian).

### Translation services

| Service | Requirements |
|---|---|
| **Bergamot** | A model for the language pair. No API key. |
| **Google Translate** | Internet. |
| **Yandex Translate** | API key and folder ID in the Translation tab. |
| **DeepL** | API key in the Translation tab. |
| **Microsoft Translator** | API key and the resource region. |
| **Custom API** | The service address and an access key if required. |

Online services receive the recognized text. When Bergamot fails, the application does not switch to an online service on its own.

The cache reduces repeated requests. In the translation window you can enable "translate only changes": unchanged text comes from the cache, and new paragraphs are translated separately, without shared context.

### Capture and overlay

LipaX uses KWin ScreenShot2, with capture through the Portal as the fallback. Overlaying the original text needs the window geometry from KWin. **With Portal capture the translation is shown in a separate window.**

A middle click on the translation window toggles pinning. In overlay mode it hides the translation; a switch in the main window brings it back. Display over a fullscreen game and placement across monitors with different scales depend on your desktop environment.

<a id="quick-start"></a>
## Quick start

1. **[Install LipaX](#installation)** and start `lipax` from the application menu or a terminal.
2. **Recognition.** In the Recognition tab choose the language of the game text. Tesseract is fine for a first run; check that the data for the language is installed.
3. **Translation.** In the Translation tab choose the target language and a service. For local translation select **Bergamot (local)**, set the source language and wait for the status **Found**. Bergamot does not detect the source language automatically.
4. **Region.** Click "Select window", then "Select region" and draw a box around the subtitles, dialogue or menu. A small region with text usually works better than the whole frame.
5. **Start.** Choose the display mode and press "Start auto-translation". Extra regions are configured in the Region tab.

If no translation appears, open "OCR view" and check whether the original is read correctly, then look at the Status tab. For RapidOCR, install ONNX Runtime and download a model in Settings first. To keep translating with the main window closed, enable "Minimize to system tray on close".

<a id="installation"></a>
## Installation

### Arch Linux package

If you have a package file, for example `lipax-1.3.5-1-x86_64.pkg.tar.zst`, install it with:

```bash
sudo pacman -U ./lipax-1.3.5-1-x86_64.pkg.tar.zst
```

The package installs the `lipax` command, the application launcher, the bundled fonts and the Bergamot engine. Translation and OCR models are downloaded separately, from the settings. After an update, quit LipaX from the tray menu and start it again.

Packages are published on the [releases page](https://github.com/GrayRats/lipax-qt/releases).

### Building from source (Arch Linux)

```bash
sudo pacman -S --needed base-devel git
git clone https://github.com/GrayRats/lipax-qt.git
cd lipax-qt
makepkg -si
```

The build downloads the Bergamot engine sources from GitHub (`mozilla/bergamot-translator` and its submodules), so it needs internet access.

To build the current project files, including local changes, use the script:

```bash
./packaging/build-local.sh -s
sudo pacman -U dist/lipax-1.3.5-1-x86_64.pkg.tar.zst
```

Additional components are installed as needed:

| Task | Install |
|---|---|
| Russian text through Tesseract | `sudo pacman -S tesseract-data-rus` |
| Another language through Tesseract | The `tesseract-data-*` package for that language. Installed languages are shown in Settings. |
| RapidOCR on the CPU | `sudo pacman -S onnxruntime-cpu`, then the model in Settings. |
| PaddleOCR | An environment as described in the [guide](docs/PaddleOCR.md) (in Russian). |

### Running a local build for development

To test KWin capture, start the application through the script:

```bash
./packaging/run-local.sh
```

The script builds the application and registers a hidden desktop entry for the local binary's path. To run an already built binary:

```bash
LIPAX_LOCAL_BINARY=/full/path/to/lipax ./packaging/run-local.sh
```

Close any other LipaX instance first: a second launch passes its commands to the running one. The installed application is not affected by this registration. To remove the local registration: `./packaging/run-local.sh --unregister`.

<a id="diagnostics-and-limitations"></a>
## Diagnostics and limitations

| Symptom | What to check |
|---|---|
| OCR sees no text or reads garbage | OCR language and data, region bounds, "OCR view", filters and the confidence threshold. |
| No Bergamot model | Connect to the network and press "Find / download model" in Settings; wait for **Found**. |
| Bergamot does not start | Check that the package with the built-in engine is installed. For the standard package the CLI path can stay empty. |
| RapidOCR reports a missing library or model | Install ONNX Runtime, download the model in the Recognition tab and press "Check again". |
| A translation service returned HTTP 429 | The request limit was reached. Wait and translate again. Automatic retries stop after 429. |
| `ScreenShot2.Error.NoAuthorized` | KWin did not allow capture for this binary. For a local build use `packaging/run-local.sh`; for an installed one restart the application after an update. |
| Overlay does not work | Check the capture source: the Portal does not report the position of the selected window. |

KWin ties the ScreenShot2 permission to the executable path from the desktop entry. The package launcher points to `/usr/bin/lipax`, not to `target/debug/lipax`. If the KDE cache is stale, run `kbuildsycoca6`. The Status tab shows whether capture is ready.

If a saved setting does not apply to an open window or to capture, quit the application completely and start it again. Check overlay compatibility with a particular fullscreen game and placement across monitors with different scales in your own environment.

To see the log when starting from a terminal:

```bash
RUST_LOG=debug lipax
RUST_LOG=debug lipax 2>&1 | tee lipax.log
```

The default level is `INFO`; `ERROR` goes to stderr, everything else to stdout.

<a id="documentation"></a>
## Documentation

Most technical documents are in Russian.

| Topic | Contents |
|---|---|
| [Bergamot](docs/Bergamot.md) | Local translation, model search, manual path and file checks. |
| [RapidOCR](docs/RapidOCR.md) | ONNX Runtime, languages and models, GPU, settings and benchmarks. |
| [MeikiOCR](docs/MeikiOCR.md) | Japanese game text, vertical text, benchmarks. |
| [PaddleOCR](docs/PaddleOCR.md) | Python environment and engine setup. |
| [Logging](docs/LOGGING.md) | Levels, components, diagnostic messages. |
| [Architecture](docs/ARCHITECTURE.md) | Application structure, capture, processing pipeline, interface. |
| [UI modernization](docs/UI-MODERNIZATION.md) | Typography, preview, settings apply policy, checks. |
| [Improvements and limitations](docs/IMPROVEMENTS.md) | Implemented changes and technical limits. |

<a id="authors"></a>
## Author and projects used

Author: **GrayRat**.

- [Lipa, the original project](https://github.com/satix-one/lipa)
- [meikipop](https://github.com/rtr46/meikipop)
- [Tesseract OCR](https://github.com/tesseract-ocr/tesseract) and [language data](https://github.com/tesseract-ocr/tessdata)
- [PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR)
- [RapidOCR](https://github.com/RapidAI/RapidOCR)
- [MeikiOCR](https://github.com/rtr46/meikiocr)
- [Bergamot Translator — Mozilla](https://github.com/mozilla/bergamot-translator)
