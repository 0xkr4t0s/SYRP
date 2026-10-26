# Plan: a native Rust, agentic-first reimplementation of JUCE

*Prepared 2026-10-26 under the placeholder codename Auricle; renamed to **Syrp** the same day by the project owner. The crate prefix is `syrp-` and the CLI is `cargo syrp`. No trademark search has been made yet. Raw sources are in `research-notes.md` in the project files. Anything marked **UNVERIFIED** could not be confirmed from a primary source.*

## 1. Executive summary

**Goal.** Build a cross-platform Rust framework with full feature parity with JUCE. AI coding agents are its primary developers and primary users. The work is done in two stages. First comes a native Rust framework with feature parity. Only after that comes an interop phase: a C ABI, a C++ facade, and Python, Swift/Kotlin, JS/wasm and C# bindings.

**Parity target.** The plan targets **JUCE 9**, not 8.x.
- JUCE 9.0.0 shipped on 2026-07-21. The current release is **9.0.4**, tagged 2026-10-12.
- The 8.x line ended at 8.0.15, plus a security-only tag `8.0.16-security` (https://github.com/juce-framework/JUCE/releases; `git ls-remote` on 2026-10-26).
- The parity ledger pins JUCE 9.0.4 as its baseline and tracks later 9.x releases.
- JUCE 9 has 25 modules, including the new `juce_audio_processors_headless` split.
- JUCE 9 still has **no CLAP** support, no sample-accurate automation and no MIDI 2.0 in its plugin API. Its "AudioProcessor v2" work is unreleased (https://forum.juce.com/t/juce-9-is-available-now/69175). Here a Rust framework can lead rather than follow.

**Core recommendations**
1. **Build the framework's spine from scratch and reuse the leaves.**
   - Build from scratch: the component and UI model, ValueTree and undo, the message loop, the processor, parameter and graph model, the plugin wrappers and the window peer layer. JUCE's semantics live here, and no Rust crate matches them.
   - Reuse mature, permissively licensed leaf crates:
     - Linebender for text and 2D: parley, fontique, harfrust, skrifa, vello_cpu and vello_gpu.
     - wgpu, AccessKit, taffy, clack (CLAP), vst3 (VST3 bindings, MIT since Steinberg relicensed the SDK), RustCrypto, rquickjs, wry and rapier2d.
     - cpal behind our own device trait to begin with.
2. **Use the plugin formats' new licences.**
   - The VST3 SDK has been **MIT since 3.8.0 (Oct 2025)**. CLAP is MIT, the AU SDK is Apache-2.0, ARA is Apache-2.0 and LV2 is ISC. All of these can be implemented natively in Rust.
   - The AAX SDK is "commercial or GPLv3", and retail Pro Tools requires PACE signing. AAX is therefore the one wrapper that gets a small C++ shim, with the SDK supplied by the user.
   - VST2 is excluded by default, because no SDK licences have been issued since about 2018.
3. **Make the framework agentic-first through deliverables, not slogans.** Every item below ships in Phase 0 and grows each phase; §2 and §13 list them.
   - One closed-loop CLI (`cargo syrp`) with JSON output and stable exit codes.
   - An MCP server and an Agent Skill that expose the same loop.
   - Deterministic headless audio and UI rendering with golden snapshots.
   - JSON introspection of component, accessibility, parameter and graph state.
   - A machine-readable API index, error-code catalogue, parity ledger and JUCE-to-Rust migration map.
4. **Prove parity with a separate differential harness built on real JUCE.**
   - JUCE is used under AGPLv3, in a separate repository that is never distributed with the framework.
   - Tests compare audio numerically and images against goldens, and run every plugin validator (pluginval, clap-validator, Steinberg's validator, auval, lv2lint and aaxval).
   - All of this is tracked in a machine-readable **parity ledger** that gates merges and phase exits.
5. **Use a strict clean room.**
   - Nobody on the project, human or agent, reads JUCE source.
   - Specs come only from public documentation and black-box behaviour.
   - A quarantined similarity scanner is the only process that touches JUCE source.
   - The `aloe-*` crates, which describe themselves as a translation of JUCE into Rust, are banned dependencies and banned references.
6. **Run a hands-off multi-agent pipeline (§12).**
   - Orchestration is deterministic and runs 10–15 implementer agents per wave against frozen interface contracts.
   - Independent reviewer agents run the tests themselves.
   - A second, adversarial reviewer is required for high-risk categories.
   - A PR-manager agent owns shared files, the merge queue and releases.
   - Every merge gate is automated. Humans act only as a defined escalation path.
7. **Licence on a revenue-gated model that is more permissive than JUCE (§10).**
   - Proposed: **`AGPL-3.0-only OR BUSL-1.1`**, where the BUSL Additional Use Grant gives free production use, including closed source, to organisations under **USD 100k** of annual revenue plus funding.
   - Each version converts to `Apache-2.0 OR MIT` after 3 years. Organisations above the threshold buy a commercial licence.
   - Agent-written code may have thin copyright protection, so the business model also rests on trademarks, human-authored contracts and docs, and paid services.
8. **Timeline.**
   - About **16–20 months to Rust feature parity** (Phases 0–7), and about 3–4 more months for interop (Phase 8).
   - This assumes 10–15 implementer agents in rolling waves, 2–4 reviewer agents, one PR-manager agent, a deterministic orchestrator and part-time human escalation. §7 explains the throughput model behind the numbers.

**What "full parity" means here.** Every public JUCE capability has a Rust equivalent that passes behavioural tests derived from JUCE's documented behaviour and from the differential harness. Parity is functional, not source-compatible, and Rust APIs are idiomatic. A small set of **deliberate gaps** is recorded in the ledger with reasons:
- VST2, because there is no SDK licence.
- AudioCDBurner and AudioCDReader, ActiveX and AppleRemote, which are legacy platform features.
- Blocking modal loops, replaced by async equivalents.
- The Projucer GUI, replaced by the CLI and MCP tooling.

## 2. What agentic-first means

"Agentic-first" means an AI agent can do four things with the framework without a human in the loop:
- discover the right API;
- write code that compiles the first time or gets an actionable error;
- verify behaviour headlessly and deterministically;
- understand failures through structured, documented signals.

Each property below is a concrete deliverable with an automated acceptance test. They serve both groups of agents: those building Syrp (§12) and those using it downstream.

| # | Deliverable | Concretely | Automated acceptance test |
|---|---|---|---|
| A1 | **AGENTS.md hierarchy** | Root `AGENTS.md` covers build and test commands, conventions, forbidden actions and the clean-room rule. Each crate has its own `AGENTS.md` covering purpose, invariants, thread affinity, RT rules, and the "how to test this crate" command. `CLAUDE.md` contains `@AGENTS.md`. AGENTS.md is now stewarded by the Linux Foundation's Agentic AI Foundation (https://openai.com/index/agentic-ai-foundation/). | A lint checks that every crate has AGENTS.md with the required headings and that the commands in it run. |
| A2 | **Machine-readable API index** | `api-index.json` is generated from rustdoc JSON (docs.rs also serves rustdoc JSON: https://docs.rs/about/rustdoc-json). Per item it records the stable path, signature, summary, thread affinity (`message`/`rt`/`any`), RT-safety, error codes returned, the JUCE equivalent(s), example IDs, feature flags and platforms. | Regenerated in CI; the diff must be empty, otherwise the PR updates it. |
| A3 | **llms.txt and Markdown mirrors** | `/llms.txt` follows the llmstxt.org v2 layout. `/llms-full.txt` holds the full concatenated docs, and every page has a `.md` mirror (https://llmstxt.org/). | Link check; a size budget per section; a freshness check. |
| A4 | **Compiling examples everywhere** | `#![deny(missing_docs)]` plus a custom lint: every public fn, type and trait has at least one doctest or a linked `examples/` program. | `cargo test --doc` plus the lint. Doc coverage is 100% for stable crates. |
| A5 | **Structured errors with stable codes** | Every crate error enum implements `Diagnostic { code(): ErrorCode, hint(): &str, docs_url() }`. Codes look like `SYRP-AUDIO-0042`, are never reused, and are catalogued in `errors/*.toml` with cause, fix, and a minimal repro. | The catalogue is generated; a test asserts every variant has a catalogue entry and that `cargo syrp explain <code>` works. |
| A6 | **Closed-loop CLI** | `cargo syrp` covers scaffold, build, test, render, snapshot, parity, validate, inspect, docs, explain and migrate (§13). Output is `--json`, with JSON Schemas published (schemars), stable exit codes, and no interactive prompts. | CLI contract tests against the schemas. |
| A7 | **MCP server** | `syrp-mcp` exposes the CLI operations as MCP tools and the indices as MCP resources. It targets MCP spec 2026-07-28 and uses the Tasks extension for long renders and builds (https://blog.modelcontextprotocol.io/tags/protocol/; rmcp 3.7). | An MCP conformance test plus golden transcripts. |
| A8 | **Agent Skill** | `skills/syrp/SKILL.md` follows the Agent Skills format (https://agentskills.io/home) and teaches the loop, recipes and failure triage. | A nightly agent-eval suite (A20) runs with the skill loaded. |
| A9 | **Deterministic headless audio** | Offline rendering of any processor or graph uses fixed block sizes, seeded RNG and a virtual transport. Results are bit-exact on the same target triple and within documented tolerances across targets. | Golden WAV and JSON comparisons in CI (§9). |
| A10 | **Deterministic headless UI** | A `ManualLoop` virtual clock and the CPU renderer (vello_cpu) render any component tree to PNG at any scale factor. Input is scripted through the accessibility tree, as egui_kittest does (crates.io `egui_kittest` 0.37). | Golden image diffs with tolerances (§9). |
| A11 | **Introspection** | `inspect` APIs and `cargo syrp inspect` dump, as JSON: the component tree (bounds, visibility, z-order, LookAndFeel and state), the accessibility tree, parameters (id, range, value, text, automation state), the processor graph, ValueTrees, device state and the timer and async-queue state. Running debug apps can open an opt-in local IPC port for live dumps. | Schema tests; snapshot tests of the dumps. |
| A12 | **Declarative definitions** | `Syrp.toml` is the project manifest and replaces `.jucer` and CMake `juce_add_plugin`. Parameter specs can be TOML/JSON or a plain builder. There is an optional declarative UI description that compiles to ordinary component code. Generated code is checked in or viewable (`cargo syrp expand`). | Round-trip tests: manifest → code → manifest. |
| A13 | **Type-state and capability tokens** | Zero-sized tokens (`MessageThread`, `RtCtx`, `PrepareCtx`) gate APIs by thread, so misuse does not compile. Builders use type-state, so a plugin cannot be built without an ID, name, vendor and bus layout. | `trybuild` compile-fail tests for every documented misuse. |
| A14 | **RT safety by construction** | The `process` signature only exposes RT-safe handles. Allocating or locking APIs require `PrepareCtx` or `MessageThread`. The allocator is guarded in debug builds (`assert_no_alloc`), and RTSan runs in CI. | RT test suite; RTSan job (§9). |
| A15 | **Minimal macro magic** | Proc-macros are limited to optional derives such as `#[derive(Params)]`, and each has a hand-written equivalent shown in the docs. There are no DSL macros in core paths. | A lint caps macro use in public API examples; `cargo syrp expand` output is snapshot-tested. |
| A16 | **Small focused files** | A soft limit of 400 lines and a hard limit of 800 per file, one primary type per file, module docs at the top, and a predictable layout (`src/<concept>.rs`, `tests/<concept>.rs`). | A size lint in CI. |
| A17 | **JUCE-name discoverability** | Every Rust item that maps to a JUCE class or method carries `#[doc(alias = "JuceName")]`, so searching rustdoc or the index for `AudioProcessorValueTreeState` finds `ParamTree`. | The alias lint is cross-checked against the migration map. |
| A18 | **JUCE→Rust migration map** | `migration-map.json` and human pages map each JUCE class, and its notable methods, to the Rust item, a pattern note, a snippet pair (JUCE idiom → Rust idiom, written fresh) and pitfalls. Exposed through `cargo syrp migrate lookup juce::Slider::setRange`. | Every ledger row with status `done` has a map entry. |
| A19 | **Capability manifest** | `capabilities.json` lists, per feature and platform, whether it is supported, partial or a gap. Agents query it before coding. It is generated from the ledger. | Freshness check. |
| A20 | **Agent usability evals** | About 60 benchmark tasks run nightly with a reference agent and the skill. Examples: "gain plugin with dB slider", "port JUCE tutorial X", "add an undoable property", "fix failing golden". Success rate, turn count and the first error code hit are tracked as KPIs. Regressions open doc or API issues automatically. | KPI thresholds gate releases. |
| A21 | **Reproducibility** | Pinned `rust-toolchain.toml` (stable is 1.99.1 as of 2026-10-13), committed lockfiles, hermetic CI images, and `cargo syrp doctor` to diagnose the environment as JSON. | Doctor runs in CI on clean images. |
| A22 | **Machine-readable change communication** | `CHANGELOG.md` plus `changes/*.json` entries, `BREAKING.md` with codemods (`cargo syrp migrate apply --from 1.3`), and semver checked by cargo-semver-checks. | Release gate. |

Design rules that follow from these deliverables:
- **One obvious way.** Each concept has one canonical API; no aliases in the API itself, only in docs.
- **Explicit over clever.** No global singletons. Where JUCE uses a singleton (MessageManager, Desktop), Syrp passes context objects or tokens.
- **Errors are values.** Only `Result` is returned. Panics are reserved for violated invariants, with code-tagged messages.
- **Everything observable.** Each subsystem has a JSON dump and a `tracing` span tree.

## 3. JUCE module surface and parity matrix

**Inventory method.** The module list comes from the `modules/` directory of JUCE master at 2026-10-12, which is tag 9.0.4. Key classes come from public header *file names* and from the public docs at docs.juce.com. No JUCE source was opened (see §10). JUCE 9 ships 25 modules:

| JUCE module | What it covers / key classes | Rust crate(s) | Reuse candidates | Phase | Difficulty |
|---|---|---|---|---|---|
| `juce_core` | **Strings and data:** String, StringArray, Identifier, `var`, DynamicObject, NamedValueSet, JSON, XmlElement/XmlDocument, Base64, LocalisedStrings, TextDiff. **Containers:** Array, OwnedArray, HashMap, SortedSet, SparseSet, AbstractFifo, ListenerList, LruCache. **Files and streams:** File, Input/OutputStream, MemoryBlock, MemoryMappedFile, ZipFile, GZIP streams. **Threads:** Thread, ThreadPool, TimeSliceThread, HighResolutionTimer, WaitableEvent, locks, ChildProcess, InterProcessLock, DynamicLibrary. **Network:** URL, WebInputStream, sockets, IPAddress, MACAddress, NamedPipe. **Maths:** BigInteger, Expression, Random, Range, NormalisableRange, StatisticsAccumulator. **System:** Time, RelativeTime, SystemStats, Uuid, Logger, WindowsRegistry, RuntimePermissions. **Testing:** UnitTest. | `syrp-var`, `-io`, `-thread`, `-process`, `-net`, `-maths`, `-sys`, `-strings`, umbrella `syrp-core` | std, serde_json, quick-xml, flate2/miniz_oxide, zip, ureq+rustls, libloading, interprocess, memmap2 (not checked), num-bigint (not checked) | 1 | M (broad, shallow). Containers map to std and are recorded in the ledger as "mapped". |
| `juce_events` | MessageManager, MessageManagerLock, `callAsync`/`callSync`, Timer, MultiTimer, AsyncUpdater, ChangeBroadcaster, ActionBroadcaster, InterprocessConnection(+Server), ChildProcessCoordinator/Worker, NetworkServiceDiscovery, JUCEApplicationBase, DeletedAtShutdown | `syrp-loop-api`, `syrp-events`, `syrp-ipc` | windows, objc2 (CFRunLoop), x11rb/rustix-style fd loops, interprocess | 1 | M–H. Integrating with host run loops in plugins is the hard part. |
| `juce_data_structures` | ValueTree, Value, CachedValue, ValueTreePropertyWithDefault, UndoManager/UndoableAction, PropertiesFile, ApplicationProperties, ValueTreeSynchroniser | `syrp-state` | serde, quick-xml, rpds (optional persistent maps) | 1 | M. Byte-compatible state migration is H. |
| `juce_cryptography` | MD5, SHA256, Whirlpool, BlowFish, RSAKey, Primes | `syrp-crypto` | RustCrypto (md-5 0.11, sha2 0.11, blowfish 0.10, whirlpool), num-bigint for JUCE-compatible raw RSA | 1 | L. The `rsa` crate carries an unpatched timing advisory, RUSTSEC-2023-0071 (https://rustsec.org/advisories/RUSTSEC-2023-0071.html). |
| `juce_osc` | OSCSender, OSCReceiver, OSCMessage, OSCBundle, OSCAddress(Pattern), OSCArgument, OSCTimeTag | `syrp-osc` | rosc 0.11.4 (wrap) | 1 | L |
| `juce_audio_basics` | AudioBuffer, FloatVectorOperations, AudioData converters, AudioChannelSet, AudioPlayHead, AudioProcessLoadMeasurer, **AudioWorkgroup**. **MIDI:** MidiMessage, MidiBuffer, MidiMessageSequence, MidiFile, MidiKeyboardState, MidiRPN. **MIDI 2.0 UMP:** packets, views, iterators, MIDI1↔2 translation, dispatcher, SysEx7. **MPE:** MPEInstrument, MPESynthesiser, MPEZoneLayout. **Sources:** the AudioSource family (Buffering, Mixer, Resampling, ChannelRemapping, ToneGenerator, Memory, IIR, Reverb). **Synthesis and DSP helpers:** Synthesiser/Voice, ADSR, SmoothedValue, Decibels, interpolators, IIRFilter, Reverb. | `syrp-audio-api`, `syrp-audio-basics`, `syrp-midi`, `syrp-mpe` | midi2 0.11 (UMP cross-check), rubato (optional high-quality resampler), audio_thread_priority (MPL-2.0) | 2 | M |
| `juce_audio_devices` | AudioDeviceManager, AudioIODevice, AudioIODeviceType. **Backends:** WASAPI, ASIO, DirectSound, CoreAudio (aggregate-device based in 9.0), ALSA, JACK, Oboe/OpenSL, iOS. **MIDI:** MidiInput/MidiOutput, **UMP endpoints, sessions and virtual endpoints**, disconnection listener (9.0.4), MidiMessageCollector. **Playback:** AudioSourcePlayer, AudioTransportSource. SystemAudioVolume. | `syrp-device-api`, `syrp-devices`, `syrp-audio-{coreaudio,wasapi,asio,alsa,jack,pipewire,aaudio,ios,web}`, `syrp-midi-io`, `syrp-midi-{coremidi,windows,alsa,android,web}` | cpal 0.18 (wrapped first), wasapi, alsa, jack, pipewire, objc2-core-audio, coremidi, midir (MIDI 1.0 fallback), windows (WinRT for Windows MIDI Services) | 2 (desktop), 6 (mobile and web) | H |
| `juce_audio_formats` | AudioFormat, AudioFormatManager, AudioFormatReader/Writer(+Options), ReaderSource, SubsectionReader, BufferingReader, MemoryMappedReader. **Codecs:** WAV (BWF, metadata, 32-bit int), AIFF, FLAC, Ogg Vorbis, MP3 (default since 9.0.2), **Opus** (9.0.3), CoreAudioFormat, WindowsMediaAudioFormat, LAME encoder (external exe). **Other:** SamplerSound/Voice, ARA readers. | `syrp-formats`, `syrp-format-{wav,aiff,flac,ogg,mp3,opus,platform}` | symphonia 0.6 (MPL-2.0, decode), claxon, lewton, flacenc, vorbis_rs, opus (libopus) | 2 | M. Metadata-chunk parity is tedious. |
| `juce_audio_processors_headless` (new in 8.0.11) | AudioProcessor, AudioPluginInstance, the parameter hierarchy (WithID, Ranged, Float/Int/Bool/Choice, Group, Hosted, Legacy), **AudioProcessorGraph**, AudioPluginFormat(Manager), PluginDescription, headless hosting of VST3/VST/AU/LV2/LADSPA, **ARA hosting**, VST2/VST3/AAX client extensions | `syrp-plugin-api`, `syrp-processors`, `syrp-host-{clap,vst3,au,lv2,ladspa,ara}` | clack-host 0.2, vst3 0.3, objc2-audio-toolbox, livi/lilv (wrap), ARA headers via bindgen | 2 | H |
| `juce_audio_processors` | AudioProcessorEditor, GenericAudioProcessorEditor, **AudioProcessorValueTreeState**, ParameterAttachments, KnownPluginList, PluginDirectoryScanner, PluginListComponent, PluginHostType, NSViewComponentWithParent | `syrp-processors` (ParamTree, scanner), `syrp-processors-ui` | none directly | 2 (headless), 4 (UI) | M–H |
| `juce_audio_plugin_client` | Wrappers for **VST3, AU v2, AUv3, AAX, LV2, Unity, Standalone** (StandaloneFilterWindow), VST2 and the ARA plugin side; VST3 moduleinfo; scale-factor utilities | `syrp-plugin`, `syrp-wrap-{clap,vst3,auv2,auv3,lv2,aax,unity,standalone,ara}`, `syrp-bundler` | clack-plugin 0.2, vst3 0.3, objc2, LV2 headers (bindgen), AAX SDK via a `cxx` shim; nice-plug and clap-wrapper as permissive design references | 4 | VH |
| `juce_audio_utils` | AudioThumbnail(+Cache), AudioVisualiserComponent, MidiKeyboardComponent, MPEKeyboardComponent, AudioDeviceSelectorComponent, AudioAppComponent, AudioProcessorPlayer, SoundPlayer, BluetoothMidiDevicePairingDialogue, AudioCDReader/Burner | `syrp-audio-utils` | none | 4 | M. CD reading and burning are a deliberate gap. |
| `juce_dsp` | AudioBlock, ProcessSpec/Context, ProcessorChain/Duplicator/Wrapper, SIMDRegister, FFT, WindowingFunction, **Convolution**, FIR/IIR + FilterDesign, SVF/TPT, LinkwitzRiley, Ladder, Ballistics, Oscillator, DelayLine, **Oversampling**, Gain, Bias, Panner, Phaser, Chorus, Compressor, Limiter, NoiseGate, Reverb, WaveShaper, DryWetMixer, LookupTable, FastMathApproximations, Matrix, Polynomial, SpecialFunctions | `syrp-dsp-{core,filters,fx,conv}`, umbrella `syrp-dsp` | rustfft 6.4 / realfft 3.5; fundsp as a reference only | 2 | M. Convolution and Oversampling latency parity is H. |
| `juce_midi_ci` | ci::Device, DeviceOptions/Features, profiles (ProfileHost/Delegate/States), Property Exchange (PropertyHost/Delegate, cache, subscriptions), Parser, Messages, encodings, MUID | `syrp-midi-ci` | midi2 (message types, cross-check only) | 2 | H. Spec-heavy; needs interop tests with real CI peers. |
| `juce_graphics` | Graphics, LowLevelGraphicsContext, software, CoreGraphics and **Direct2D** renderers. Colour, gradients (incl. two-point conical), FillType, Path, PathStrokeType, StrokeOptions, EdgeTable. Geometry (AffineTransform, Rectangle(List), Point, Line, Parallelogram, BorderSize, Justification, RectanglePlacement). Image, ImageCache, codecs (PNG, JPEG, GIF, **WebP** 9.0.3), effects. Font, FontOptions, **FontFeatures**, **variable fonts** (9.0), Typeface, AttributedString, TextLayout, GlyphArrangement, ShapedText/JustifiedText (HarfBuzz and SheenBidi). Drawable* plus a **new SVG parser** (9.0). | `syrp-geom`, `syrp-gfx-api`, `syrp-gfx`, `syrp-image`, `syrp-text`, `syrp-drawable`, `syrp-render-cpu`, `syrp-render-gpu` | kurbo, peniko, vello_cpu/vello_gpu 0.3, wgpu 30, parley/fontique 0.12, harfrust 0.15, skrifa 0.49, usvg 0.49, image, png, zune-jpeg, image-webp | 3 | VH |
| `juce_gui_basics` | **Component tree:** Component, ComponentPeer, Desktop, Displays, CachedComponentImage. **Windows:** Top-level, Resizable, Document, Dialog, Alert, CallOut, Tooltip, NativeMessageBox, ScopedMessageBox. **LookAndFeel V1–V4.** **Widgets:** Button family, Slider, ComboBox, Label, TextEditor, ListBox, TableListBox, TreeView, Toolbar, ProgressBar. **Menus:** PopupMenu, MenuBar, BurgerMenu. **Layout:** FlexBox, Grid, StretchableLayout, Concertina, Tabbed, Viewport, ScrollBar, SidePanel, MDI, constrainers, resizers, ComponentAnimator. **Input:** Mouse/Key/Touch events, MouseInputSource, KeyPress, TextInputTarget, focus traversers. **Drag and drop**; **Commands:** ApplicationCommandManager, KeyPressMappingSet. **File browser:** FileChooser, FileBrowserComponent. **Properties:** PropertyPanel. **Accessibility:** handlers, roles, actions, interfaces. **Other:** ContentSharer, SystemClipboard, MouseCursor, VBlankAttachment, JUCEApplication, RelativeCoordinate (legacy). | `syrp-peer-api`, `syrp-peer-{win32,appkit,x11,wayland,uikit,android,web}`, `syrp-ui`, `syrp-ui-layout`, `syrp-a11y`, `syrp-commands`, `syrp-laf`, `syrp-widgets-{buttons,slider,text,lists,menus,panels}`, `syrp-windows`, `syrp-filebrowser`, `syrp-app` | taffy 0.15 (Flex/Grid), accesskit 0.25, raw-window-handle, keyboard-types, cursor-icon, smithay-client-toolkit, x11rb; baseview and winit as references | 3 | VH |
| `juce_gui_extra` | CodeEditorComponent, CodeDocument, tokenisers (C++, Lua, XML). **WebBrowserComponent** with relays (Slider/Toggle/ComboBox), WebControlParameterIndexReceiver, resource provider, native functions, npm `@juce-framework/webview`. ColourSelector, KeyMappingEditor, PreferencesPanel, RecentlyOpenedFilesList, SplashScreen, SystemTrayIcon, PushNotifications, AnimatedAppComponent, BubbleMessage, LiveConstantEditor, FileBasedDocument. **Native embedding:** HWND, NSView, UIView, XEmbed, AndroidView, ActiveX. AppleRemote. | `syrp-ui-extra`, `syrp-code-editor`, `syrp-webview`, `syrp-embed` | wry 0.57, webview2-com, webkit2gtk | 5 | H. WebView inside plugins, especially on Linux, is the hard case. |
| `juce_opengl` | OpenGLContext (attached to a Component, own render thread), OpenGLRenderer, OpenGLAppComponent, FrameBuffer, Image, GraphicsContext, ShaderProgram, Texture, PixelFormat, Matrix3D, Quaternion, Vector3D, Draggable3DOrientation, GL/GLES loaders | `syrp-opengl` | glow 0.18, glutin 0.32 (apps), own child-window context creation | 3 (desktop), 6 (mobile) | M–H. OpenGL is deprecated on macOS. |
| `juce_video` | VideoComponent (playback), CameraDevice (capture, preview, recording) | `syrp-video` | objc2-av-foundation, windows (Media Foundation), gstreamer (LGPL runtime), JNI; nokhwa as a reference | 5–6 | M–H |
| `juce_box2d` | Bundled Box2D 2.x world plus Box2DRenderer | `syrp-physics2d` | rapier2d 0.36 (Apache) | 5 | L–M. Documented deviation: the physics engine's API differs from Box2D's. |
| `juce_product_unlocking` | OnlineUnlockStatus, OnlineUnlockForm, KeyFileGeneration, TracktionMarketplaceStatus, **InAppPurchases** | `syrp-unlock`, `syrp-unlock-ui` | num-bigint, objc2 (StoreKit 2), JNI (Play Billing) | 1 (key files), 5–6 | M |
| `juce_analytics` | Analytics, AnalyticsDestination, ThreadedAnalyticsDestination, ButtonTracker | `syrp-analytics` | ureq | 1 (headless core), 5 (destinations) | L |
| `juce_animation` | Animator, ValueAnimatorBuilder, AnimatorSetBuilder, AnimatorUpdater, VBlankAnimatorUpdater, Easings (incl. cubic Bézier) | `syrp-animation` | none (kurbo for Bézier maths) | 3 | L–M |
| `juce_javascript` | JavascriptEngine (QuickJS via CHOC since 8.0.4), JSObject, JSCursor | `syrp-js` | rquickjs 0.14 (MIT); boa_engine as a pure-Rust option | 5 | L–M |

**Tooling parity.** These are not modules, but they are part of what JUCE users get.

| JUCE tool | Syrp equivalent | Phase |
|---|---|---|
| Projucer (project generator, exporters, editor) | `Syrp.toml` plus `cargo syrp new/add/build/bundle`. Exporters are unnecessary because Cargo is the build system. The GUI is replaced by CLI and MCP (deliberate gap). | 0 → 4 |
| CMake API (`juce_add_plugin`, `juce_add_gui_app`, `juce_add_binary_data`, `juce_add_pip`, …) and juceaide | Manifest keys plus a `build.rs` helper crate `syrp-resources` (BinaryBuilder equivalent). The VST3 manifest step, plist and entitlements are handled by `syrp-bundler`. | 0, 4 |
| AudioPluginHost | `syrp-host-app` (GUI plus headless mode) | 4 |
| DemoRunner (about 90 demos) | `syrp-demos`; each demo doubles as a golden test | 3–6 |
| UnitTest / UnitTestRunner | `cargo test`/nextest plus `syrp-testkit` | 0 |
| AudioPerformanceTest, NetworkGraphicsDemo | `syrp-perf` (benchmarks); NetworkGraphicsDemo is a low-priority demo | 5 |

**Hardest parity items, in rough order of risk**
1. **Text layout and TextEditor.**
   - JUCE 8 and 9 shape text with HarfBuzz and SheenBidi and add font features and variable fonts. Syrp uses harfrust, the Rust port of HarfBuzz matching HarfBuzz 15.0.0 (https://github.com/harfbuzz/harfrust).
   - With the same font and features, glyph IDs and advances should match exactly. That is an objective test.
   - Line breaking, justification, caret and selection behaviour, IME, and TextEditor's undo and paste behaviour have to be specified from observation.
2. **Rendering fidelity.**
   - JUCE's software renderer antialiases with edge tables, and its Direct2D and CoreGraphics backends each differ slightly.
   - Syrp targets "visually equivalent within tolerance", not pixel-identical output (§9). Hairlines, pixel snapping at fractional scale factors, and gradients need careful specs.
3. **Component semantics.** Mouse-event ordering (enter, exit, drag and double-click timing), focus and keyboard routing, modal handling and repaint coalescing all need specs. JUCE's blocking `runModalLoop` is replaced by async modals, a deliberate deviation.
4. **Plugin wrapper edge cases.**
   - VST3: parameter threading and bus arrangement negotiation. AU: property listeners and parameter-tree updates.
   - Hosts: host-specific quirks (JUCE's PluginHostType encodes many of them), resizing and scale factors, and state restore during processing.
   - Every quirk has to be discovered by testing in real DAWs.
5. **State compatibility.**
   - Users porting a JUCE plugin need old sessions to load. That means reading JUCE-produced plugin state blobs, which carry ValueTree XML and binary formats inside format-specific wrappers.
   - Syrp learns these formats as black boxes: it generates blobs with the harness and fits a reader to the output. It never reads JUCE's serializer source (§10).
6. **AudioProcessorGraph.** This covers rebuild semantics under live audio, latency compensation and MIDI routing. Syrp adds an RT-safe graph swap.
7. **CoreAudio aggregate devices** (9.0), **WASAPI low-latency and exclusive modes**, **ASIO**, and **Windows MIDI Services**. Windows MIDI Services has been generally available in Windows 11 since 2026-02-17, rolling out in phases (https://blogs.windows.com/windowsexperience/2026/02/17/making-music-with-midi-just-got-a-real-boost-in-windows-11/).
8. **MIDI-CI** profiles and property exchange, which need interoperability with Apple, Windows and hardware peers.
9. **WebView inside plugins.** On Linux, JUCE runs WebKitGTK through a subprocess helper (CMake `juce_link_with_embedded_linux_subprocess`). Syrp needs the same design.
10. **AAX** (C++ SDK, PACE signing), **ARA**, **AUv3** packaging, and mobile app lifecycles.
11. **Wayland.** JUCE only has a preview branch (9.0.3). Embedding plugin UIs under Wayland has no cross-host standard (UNVERIFIED), so plugin UIs on Linux target X11/XWayland first.

## 4. Rust ecosystem: reuse, wrap, or build

**Decision vocabulary**

| Decision | Meaning |
|---|---|
| **Reuse** | Depend on the crate directly. |
| **Wrap** | Depend on it behind an Syrp trait so it can be replaced later. |
| **Fork** | Vendor the code into the workspace and own it, keeping attribution. Used only for permissive licences. |
| **Build** | Write from scratch. |
| **Reference** | Read the code for design ideas only. Permissive licences only. |
| **Avoid** | Do not use. |

All data below comes from the crates.io API on 2026-10-26: version, last publish date and licence. Repository commit dates come from metadata-only clones (see `research-notes.md`).

**Cross-cutting policies**
1. **Dependency licences.**
   - Allowed: MIT, Apache-2.0, BSD, ISC, Zlib, Unicode-3.0 and MPL-2.0. MPL-2.0 is file-level copyleft, which is fine for a dependency.
   - GPL and LGPL code may only appear behind optional features that load at runtime: GStreamer, LAME, and the user-supplied ASIO and AAX SDKs.
   - Non-OSI dependencies are denied. `cargo-deny` enforces this.
2. **Upstreaming.**
   - The RustAudio community AI policy says that "vibe coding" entire pull requests is unacceptable, and that suspected low-quality AI PRs are closed (https://rust.audio/community/ai/). This covers cpal, baseview, nice-plug and others. Agents must therefore never open upstream PRs.
   - If a fix is needed: carry a fork, file a human-written issue, and contribute human-authored patches where the policy allows.
3. **Isolation.** Every third-party dependency that sits on a public API boundary is wrapped, so it can be swapped without semver breaks.
4. **Banned.** The `aloe-*` crates, a GPL "translation" of JUCE (https://lib.rs/crates/aloe-vst), and any other code derived from JUCE.

### 4.1 Audio I/O
| Candidates | Maturity / licence | Decision | Rationale |
|---|---|---|---|
| **cpal** 0.18.2 (Aug 2026; main branch active Oct 2026) | Very mature; Apache-2.0. Hosts: WASAPI, ASIO, JACK, CoreAudio, ALSA, PipeWire, PulseAudio, AAudio, WebAudio and AudioWorklet. Duplex support is unreleased on main. | **Wrap** first, then replace per backend | It gets audio running on every platform by Phase 2. It does not expose JUCE-level controls: exclusive or low-latency WASAPI, aggregate devices, channel names, iOS session routing. The README lists a **macOS 14.2 minimum** for its CoreAudio backend, which conflicts with a broad macOS target. The reason for that floor is UNVERIFIED. |
| wasapi 0.26, alsa 0.12, jack 0.13, pipewire 0.10, objc2-core-audio / objc2-audio-toolbox 0.3.2, coreaudio-rs 0.14, asio-sys 0.4 | Maintained; MIT or Apache | **Reuse** as the low-level bindings under native Syrp backends | Building blocks for the parity backends. |
| interflow (git only, Jun 2026) | Early; WASAPI, ALSA, PipeWire, CoreAudio | **Reference** | Good design ideas for duplex and separate input/output devices. Not published to crates.io and missing ASIO and JACK. |
| oboe 0.6.1 (2024) | Stale | **Avoid**. Build an AAudio backend on `ndk`. | No releases since 2024. |
| audio_thread_priority 0.38 | MPL-2.0 | **Reuse** | RT thread promotion. Apple workgroup joining is implemented separately with objc2. |

### 4.2 MIDI
| Candidates | Maturity / licence | Decision | Rationale |
|---|---|---|---|
| midir 0.11.1 (Oct 2026) | Mature; MIT; MIDI 1.0 only | **Wrap** as a Phase 2 fallback | Lets MIDI 1.0 I/O work early. It has no UMP or MIDI 2.0. |
| coremidi 0.9.2, objc2-core-midi, alsa (seq and UMP), windows (WinRT projection for Windows MIDI Services), ndk (AMidi) | Maintained | **Reuse** under a native `syrp-midi-io` | Gets UMP endpoints, virtual ports and disconnection events in line with JUCE 9. |
| midi2 0.11.1 (bl-midi2-rs) | MIT/Apache, active | **Reference** and cross-check tests | Typed UMP and CI messages. Syrp's `syrp-midi` owns its types so it can control RT-safety and its API. |
| wmidi 4.0, midly 0.5 (2023), midi-msg 0.9 | Various | **Reference** only (midly as a MidiFile fuzz oracle) | Behaviour must match JUCE's MidiFile, so Syrp builds its own. |
| (none found) | No Rust MIDI-CI device stack | **Build** `syrp-midi-ci` | A gap in the ecosystem. |

### 4.3 Plugin formats and frameworks
| Candidates | Maturity / licence | Decision | Rationale |
|---|---|---|---|
| **clack** (clack-plugin, clack-host, clack-extensions 0.2.0, Sep 2026) | Safe, low-level, active; MIT/Apache | **Reuse** for the CLAP wrapper and CLAP hosting | Its README notes that no other Rust CLAP host layer exists. |
| **vst3** 0.3.0 (coupler-rs, Dec 2025) | Bindings generated from the now-MIT VST3 headers; MIT/Apache | **Reuse** for the VST3 wrapper and hosting | The permissive replacement for the GPLv3 `vst3-sys`, which last changed in 2023. |
| nih-plug (git, ISC) | **"Currently in maintenance mode"** per its README | **Reference** | Shows mature designs: parameter smoothing, CLAP polyphonic modulation, sample-accurate automation. |
| **nice-plug** 0.4.2 (Codeberg, ISC, Oct 2026) | The community fork of nih-plug. Supports VST3, CLAP and standalone. Described as experimental. | **Reference** | Same ideas, but its API shape is not JUCE parity, and it has no AU, AAX or LV2. |
| coupler (git, MIT/Apache) | "Not production-ready" | **Reference** | Its type-level threading-contract ideas are useful. |
| truce 6.3.0 | **Custom non-OSI licence** (`LicenseRef-TruceLicense-1.0`) | **Avoid** | Licence incompatible. Claims every format, but maturity is unverified. |
| beamer 0.2.4 (MIT), rack 0.4.8 (MIT/Apache, AU hosting), truce-rack 1.1.5 (MIT/Apache, host) | Early | **Reference** | Useful for AU hosting details. |
| rust-lv2 (lv2, lv2-sys: last published 2020) | Dead | **Build** an LV2 wrapper from the ISC LV2 headers with bindgen | No maintained alternative exists. |
| livi 0.9.0 (MIT, Oct 2026; uses lilv) | Active | **Wrap** for LV2 hosting, then replace lilv (C, ISC) with a Rust Turtle reader in Phase 7 | Native Rust first, but pragmatic early on. |
| clap-wrapper (C++, MIT) / clap-wrapper-rs 0.3.1 | Mature C++ | **Reference**, plus an optional bridge during the early phases | Shows how to bridge CLAP to AU and AAX. A C++ dependency is not acceptable as a final architecture. |
| AU v2/v3 | Apple AudioUnitSDK is Apache-2.0 v1.4.0 (https://github.com/apple/AudioUnitSDK) | **Build** with objc2 and objc2-audio-toolbox | There is no maintained permissive Rust AU wrapper. |
| AAX | SDK under "commercial or GPLv3" (LICENSE in JUCE's bundled copy); PACE signing required | **Build** a thin `cxx` shim over the user-supplied SDK | The SDK is C++. This is the one sanctioned C++ component. |
| ARA | ARA SDK Apache-2.0 (https://www.synthtopia.com/content/2021/05/13/celemony-releases-ara-sdk-under-open-source-license/) | **Build** with bindgen over the C API | A C API, so it binds cleanly. |
| VST2 | No SDK licences since about 2018; `vst` crate deprecated | **Avoid** by default (deliberate gap) | Revisit only if a user holds a valid licence (§11 decision). |

### 4.4 Windowing
| Candidates | Maturity / licence | Decision | Rationale |
|---|---|---|---|
| baseview 0.4.1 (Oct 2026) | Plugin-oriented: embeds in a host parent on Win32, Cocoa and X11. MIT/Apache. The RustAudio AI policy applies to contributions. | **Fork** ideas and code into `syrp-peer-{win32,appkit,x11}` | Plugin embedding is mandatory and the peer layer is JUCE's ComponentPeer. Syrp must own it fully: IME, DPI, cursors, drag and drop, multi-touch, VBlank, accessibility hooks. |
| winit 0.30.13 (Oct 2026) | Very mature; Apache-2.0 | **Wrap** for `syrp-peer-{uikit,android,web}` app lifecycles; **Reference** for desktop | It owns the event loop, which cannot coexist with a host-owned run loop in plugins, so it is not suitable for desktop plugin peers. Good for mobile and web standalone apps. |
| raw-window-handle 0.6, softbuffer 0.4, smithay-client-toolkit 0.21, x11rb 0.14, windows 0.62, objc2-app-kit / ui-kit 0.3.2, android-activity 0.6, jni 0.22, web-sys | Mature | **Reuse** | Platform bindings. |
| keyboard-types 0.8, cursor-icon 1.2, ui-events 0.3 | Mature or early | **Reuse** (keyboard-types, cursor-icon); **Reference** (ui-events) | Shared vocabulary types. |

### 4.5 GUI toolkits
| Candidates | Maturity / licence | Decision | Rationale |
|---|---|---|---|
| egui 0.37 (MIT/Apache), iced 0.14 (MIT), vizia 0.4 (MIT), xilem/masonry 0.4 (Apache), floem 0.2 (MIT), dioxus 0.7 | Varied maturity | **Build** a JUCE-semantics component model. **Reference** masonry (retained widget tree plus testing harness), vizia (plugin GUI) and egui_kittest (AccessKit-driven tests). Offer optional adapters such as "host an egui UI in an Syrp editor" in Phase 5. | None of them provides JUCE's Component, LookAndFeel and Graphics paint model, which is the part users and agents port. Parity requires owning that model. |
| slint 1.18 | GPL-3.0 or royalty-free or commercial | **Avoid** in core | Licence. |
| **taffy** 0.15.1 (Oct 2026, MIT) | Mature flexbox and grid | **Reuse** inside `syrp-ui-layout` for FlexBox and Grid | Matching CSS-like semantics; JUCE's FlexBox and Grid follow CSS. |

### 4.6 2D graphics and GPU
| Candidates | Maturity / licence | Decision | Rationale |
|---|---|---|---|
| **vello_cpu** 0.3.0 (Oct 2026) | Per Vello's README, "the most mature choice" for CPU rendering. Multithreaded with SIMD. Apache/MIT. | **Wrap** as `syrp-render-cpu`, the reference renderer used for goldens and as the fallback | Deterministic CPU output for headless tests. A single-threaded scalar mode is pinned for goldens (spike in Phase 0). |
| **vello_gpu** 0.3.0 (formerly vello_hybrid) | Intended to become the primary GPU renderer. wgpu plus WebGL2, no compute shaders needed. | **Wrap** as `syrp-render-gpu` | Broad GPU compatibility, including web. |
| vello 0.11 (compute renderer) | Described upstream as experimental | **Avoid** for now | Maturity. |
| tiny-skia 0.12 (BSD-3) | Mature Skia subset | **Fallback** candidate, decided in the Phase 0 spike | Very stable. Used by resvg. Single-threaded. |
| skia-safe 0.153 (MIT bindings to Skia) | Mature but heavy C++ | **Avoid** in core; possible optional backend | A large C++ dependency; against native-first. |
| femtovg 0.27 | GL-based canvas | **Avoid** | Vello covers this. |
| wgpu 30.0.1, kurbo 0.13, peniko 0.6, lyon 1.0 | Mature | **Reuse** (wgpu, kurbo, peniko); lyon as a reference | kurbo's Bézier paths map onto JUCE Path; peniko provides brushes. |
| resvg/usvg 0.49 | Mature SVG | **Reuse** usvg to parse SVG into a Drawable tree | Matches JUCE 9's improved SVG coverage (gradients, clip paths, blend modes). |
| image 0.25, png 0.18, zune-jpeg 0.5, image-webp 0.2 | Mature | **Reuse** in `syrp-image` | Codecs. |

### 4.7 Text
| Candidates | Maturity / licence | Decision | Rationale |
|---|---|---|---|
| **parley** 0.12 + **fontique** 0.12 (Oct 2026) | Rich text layout and system font enumeration with fallback. Apache/MIT. Stack: Fontique, HarfRust, Skrifa, ICU4X. | **Wrap** in `syrp-text` | JUCE's AttributedString, TextLayout and GlyphArrangement APIs sit on top; Syrp owns line breaking where parity needs it. |
| **harfrust** 0.15 (MIT, Oct 2026) | HarfBuzz port matching HarfBuzz 15.0.0 | **Reuse** (through parley) | Same shaping semantics as JUCE's embedded HarfBuzz. |
| skrifa 0.49 / read-fonts 0.46 | Google Fonts "oxidize" stack | **Reuse** | Outlines, hinting, variable-font axes (JUCE 9 parity). |
| rustybuzz 0.20 (2024), swash 0.2, cosmic-text 0.19, ttf-parser, fontdb | Varied | **Avoid** (rustybuzz is superseded); swash and cosmic-text as references | Keep a single font stack. |
| icu_segmenter / icu_properties 2.4, unicode-bidi 0.3 | Mature | **Reuse** | Line and grapheme breaking, bidi (JUCE uses SheenBidi). |

### 4.8 Audio formats
| Candidates | Maturity / licence | Decision | Rationale |
|---|---|---|---|
| symphonia 0.6.1 (Aug 2026) | Mature decoders; **MPL-2.0** | **Wrap** for MP3, AAC and ALAC decoding | MPL is acceptable as a dependency. It sits behind the `AudioFormat` trait. |
| hound 3.5 (2023), claxon 0.4 (2020), lewton 0.10 (2021) | Stable but old | **Build** WAV and AIFF (metadata parity: BWF, cue, smpl, acid, iXML); **Wrap** claxon and lewton for decoding initially | JUCE's WAV metadata surface is large, so Syrp needs to own it. |
| flacenc 0.5 (Apache), vorbis_rs 0.5 (BSD-3, libvorbis), opus 0.4 (libopus) | Maintained | **Wrap** for encoders | Pure-Rust Vorbis and Opus encoders are immature, so C libraries are accepted here as a documented exception. |
| mp3lame-encoder 0.2 | **LGPL-3.0** | **Avoid** linking. Mirror JUCE: LAME runs as an external executable. | Licence. |
| CoreAudio / Media Foundation decoders | Platform | **Build** `syrp-format-platform` | Parity for CoreAudioFormat and WindowsMediaAudioFormat. |

### 4.9 DSP
| Candidates | Maturity / licence | Decision | Rationale |
|---|---|---|---|
| rustfft 6.4 / realfft 3.5 | Mature | **Wrap** inside `syrp-dsp-core::Fft` | The FFT is a solved problem. Syrp wraps it to match JUCE's API for scaling and ordering. |
| rubato 5.0 (Oct 2026) | Mature resampling | **Reuse** for the high-quality resampler option | JUCE's ResamplingAudioSource and interpolators are rebuilt to their documented behaviour. |
| fundsp 0.23, dasp 0.11 (2020), biquad 0.6, idsp 0.23 | Varied | **Reference** only | The filters, oscillators and effects are rebuilt from public-domain DSP literature (RBJ cookbook, Zavalishin's TPT, and so on) to match JUCE's documented responses. |
| SIMD | `std::simd` is still unstable (UNVERIFIED for 1.99); `wide` / `pulp` were not checked | **Build** `SimdRegister<T>` over `core::arch` with a scalar fallback; evaluate `wide` in Phase 0 | Needed for SIMDRegister parity and FloatVectorOperations speed. |

### 4.10 Real-time primitives
| Candidates | Maturity / licence | Decision | Rationale |
|---|---|---|---|
| rtrb 0.4.1 (Oct 2026), ringbuf 0.5 | Mature SPSC queues | **Reuse** rtrb | Event and message queues between threads. |
| basedrop 0.1.3 | Small, stable | **Fork** into `syrp-rt` (deferred drop collector) | A core RT pattern; Syrp needs to own it. |
| assert_no_alloc 1.1.2 (2021) | Stable, tiny | **Fork** into `syrp-rt` (debug allocator guard) | Unmaintained but trivial. |
| rtsan-standalone 0.3 (Apache); rustc RTSan (rust-lang/rust#147935; nightly status UNVERIFIED) | New | **Reuse** in CI | Catches blocking calls the type system cannot see. |
| crossbeam 0.8.5, arc-swap 1.9, triple_buffer 9.0 (MPL-2.0), atomic_float 1.1, loom 0.7.2 | Mature | **Reuse**. arc-swap is used only with deferred reclamation; loom is used in tests. | |

### 4.11 Data, state and serialisation
| Candidates | Maturity / licence | Decision | Rationale |
|---|---|---|---|
| serde 1.0.229, serde_json 1.0.154, quick-xml 0.42, roxmltree 0.21 | Very mature | **Reuse** | JSON and XML backends for `var`, XmlElement and ValueTree. |
| rpds 1.2 (MIT, 2026) vs im 15.1 (2022, MPL-2.0+) | rpds is maintained | **Reuse** rpds, optionally for snapshotting | Cheap undo snapshots and diffing. |
| undo 0.52 | Generic | **Reference** | JUCE's UndoManager transaction semantics need a custom design. |

### 4.12 Networking, OSC and IPC
| Candidates | Maturity / licence | Decision | Rationale |
|---|---|---|---|
| ureq 3.5 + rustls 0.23 | Mature, synchronous | **Reuse** for URL and WebInputStream | Matches JUCE's blocking-stream model without needing tokio in the core. |
| reqwest 0.13 / tokio 1.54 | Mature async | **Optional** feature | Available to async users. Not pulled into the core. |
| rosc 0.11.4 (Mar 2025) | Mature | **Wrap** | OSC packet codec. Syrp builds the sender, receiver and pattern matching to match JUCE semantics. |
| interprocess 2.4, mdns-sd 0.21 | Mature | **Reuse** interprocess; **Reference** mdns-sd | NamedPipe and InterprocessConnection. JUCE's docs describe NetworkServiceDiscovery as repeatedly broadcasting a service's existence (UDP broadcast is assumed; UNVERIFIED). Syrp builds its own. |

### 4.13 Crypto
| Candidates | Maturity / licence | Decision | Rationale |
|---|---|---|---|
| RustCrypto: md-5, sha2, blowfish, whirlpool | Mature | **Reuse** | Hash and cipher parity. |
| rsa 0.9.10 | RUSTSEC-2023-0071, no patch (Sep 2026) | **Avoid**. Use num-bigint for JUCE-compatible raw RSAKey operations (used for key files), and document that these operations are not for network-facing use. Recommend ed25519-dalek 3.0 for new licensing schemes. | Security. |
| ring 0.17 / aws-lc-rs / rustls | Mature | **Reuse** rustls for TLS | |

### 4.14 Physics
| Candidates | Maturity / licence | Decision | Rationale |
|---|---|---|---|
| rapier2d 0.36.1 (Oct 2026, Apache) | Mature | **Wrap** in `syrp-physics2d` with a Box2DRenderer-style debug renderer | box2d-rs is dead (2023). Parity here means equivalent capability; Box2D's API is not reproduced. |

### 4.15 WebView and JavaScript
| Candidates | Maturity / licence | Decision | Rationale |
|---|---|---|---|
| **wry** 0.57.1 (Oct 2026) | Mature (Tauri). Backends: WebView2, WKWebView, WebKitGTK, Android WebView. Apache/MIT. | **Wrap** in `syrp-webview` | Covers every JUCE WebBrowserComponent backend. Child-window embedding goes through raw-window-handle. On Linux the plugin case runs in a subprocess, as JUCE does. |
| webview2-com 0.39, webkit2gtk 2.0 | Mature | **Reuse** for features wry does not expose (resource interception, native function bridges) | Parity for JUCE 8's ResourceProvider and native functions. |
| rquickjs 0.14 (MIT, QuickJS) | Mature | **Wrap** in `syrp-js` | Same engine family as JUCE's QuickJS-via-CHOC. |
| boa_engine 0.22 (pure Rust) | Maturing | Optional backend | A pure-Rust option. |

### 4.16 Video and camera
| Candidates | Maturity / licence | Decision | Rationale |
|---|---|---|---|
| objc2-av-foundation, windows (Media Foundation), JNI (MediaPlayer, Camera2) | Platform bindings | **Build** native backends | JUCE uses native APIs; this avoids codec licensing. |
| gstreamer 0.25 (bindings MIT/Apache; runtime LGPL) | Mature | **Wrap** behind an optional feature on Linux | JUCE has no Linux video either; this goes beyond parity. |
| ffmpeg-next 9.0, nokhwa 0.10 | Mature-ish | **Avoid** (ffmpeg licensing); **Reference** (nokhwa) | |

### 4.17 OpenGL
| Candidates | Maturity / licence | Decision | Rationale |
|---|---|---|---|
| glow 0.18 | Mature | **Reuse** | GL function loading. |
| glutin 0.32 (Apr 2025) | Mature, but tied to winit-style ownership | **Reuse** for standalone apps only. **Build** child-window context creation (WGL, CGL/NSOpenGLContext, GLX/EGL), following raw-gl-context, which went stale in 2022. | Plugins need GL in host-owned child windows. |

### 4.18 Accessibility
| Candidates | Maturity / licence | Decision | Rationale |
|---|---|---|---|
| **AccessKit** 0.25.2 (Oct 2026) | Adapters: Windows UIA, macOS, Unix AT-SPI, Android, iOS. Web planned. MIT/Apache. | **Reuse** via `syrp-a11y` | Matches JUCE's AccessibilityHandler model: roles, actions, value, text and table interfaces. Its tree also drives Syrp's UI test scripting. |

### 4.19 Tooling (tests, docs, release, agents)
| Candidates | Decision | Rationale |
|---|---|---|
| cargo-nextest 0.9, insta 1.49, trybuild 1.0, proptest 1.12, cargo-fuzz 0.13 / arbitrary, cargo-mutants 27, criterion 0.8 / divan | **Reuse** | Test stack. |
| cargo-public-api 0.52, rustdoc-types 0.61, rustdoc-json 0.9, cargo-semver-checks 0.51 | **Reuse** | Contract freezing, API index, semver gates. |
| cargo-deny 0.20, cargo-vet 0.10, cargo-audit 0.22, cargo-about 0.9, cargo-auditable 0.7 | **Reuse** | Licence and supply-chain gates, SBOM. |
| release-plz 0.4, git-cliff 2.14, cargo-dist 0.32 | **Reuse** | Automated releases (§12). |
| mdbook 0.5.4, lychee 0.24 | **Reuse** | Docs site and link checking (§13). |
| rmcp 3.7 (official MCP SDK), clap 4.6 (CLI), schemars 1.2, miette 7.6 / thiserror 2.0 | **Reuse** | CLI, MCP server, JSON Schemas, diagnostics. |
| pluginval 2.0.0 (GPL-3.0, run as an external tool), clap-validator 0.4.1 (MIT), Steinberg validator (MIT SDK), auval, lv2lint (licence UNVERIFIED), aaxval (AAX SDK) | **Reuse** as external CI tools | Validators (§9). |
| cbindgen, cxx/autocxx, pyo3/maturin, uniffi, wasm-bindgen, diplomat, csbindgen/interoptopus | **Reuse** in Phase 8 | §8. |

## 5. Architecture and crate layout

### 5.1 Principles that make parallel waves possible
1. **Contracts first.** Every boundary that more than one work package touches is a small `*-api` crate of traits, data types and error enums with no implementation. These crates are written in Phase 0 and **frozen**:
   - a `cargo public-api` snapshot is committed under `contracts/`;
   - CI fails on any diff unless the PR is a contract-change work package approved by the adversarial reviewer (§12).
2. **Leaves implement contracts, not each other.** Each codec, audio backend, MIDI backend, window peer, host format and plugin wrapper depends only on contract crates, plus foundation and core. The 7 codecs, 9 audio backends, 7 peers and 9 wrappers can therefore be built simultaneously.
3. **One crate per work package owner.** Crates are small, typically 2–8k lines. That keeps work packages reviewable and makes agent ownership unambiguous.
4. **The umbrella facade** `syrp` re-exports everything behind cargo features named after JUCE modules (`core`, `events`, `audio-basics`, `gui-basics`, …). This keeps the migration map simple.

### 5.2 Workspace layout
```text
syrp/
  Cargo.toml  rust-toolchain.toml  deny.toml  ownership.toml   # shared files: PR manager only
  AGENTS.md  CLAUDE.md  BREAKING.md  CHANGELOG.md
  contracts/<crate>.public-api.txt     # frozen API snapshots
  parity/{ledger.json, ledger.schema.json, scenarios/}       # parity ledger + differential scenarios
  errors/<domain>.toml                 # error-code catalogue (single source)
  docs/{guide,cookbook,migration,design}/ + book.toml        # human docs source (mdBook)
  skills/syrp/{SKILL.md, references/, scripts/}           # Agent Skill
  work/{packages/WP-*.toml, waves/*.toml}                    # machine-readable work packages
  crates/<group>/<crate>/{Cargo.toml, AGENTS.md, src/, tests/, examples/, benches/}
  apps/{host-app, demos, perf}
  xtask/                               # repo automation (lints, generators)
```

| Layer | Crates (one-line purpose) |
|---|---|
| **L0 Foundation** | `syrp-error`: `ErrorCode`, the `Diagnostic` trait, hints and docs URLs. `syrp-rt`: RT tokens, `RtSafe` marker, SPSC queues (rtrb), deferred-drop collector, `AtomicF32`/`AtomicF64`, allocation guard. `syrp-geom`: Point, Rect, RectList, AffineTransform, BorderSize, Line, Parallelogram, Justification, RectanglePlacement (kurbo inside). |
| **L1 Contracts** (frozen in Phase 0) | `syrp-loop-api`: MessageLoop trait, tasks, timers, VBlank source. `syrp-audio-api`: buffer views, ChannelSet, BusLayout, ProcessConfig, PlayHead and transport. `syrp-midi`: MidiMessage, MidiBuffer, UMP packet types (data types plus codecs). `syrp-format-api`: AudioFormat, Reader and Writer traits, metadata model. `syrp-device-api`: audio and MIDI device backend traits. `syrp-plugin-api`: Processor, ParamSpec, Editor, PluginDescriptor, state traits. `syrp-gfx-api`: Graphics ops, DisplayList, Brush, Path, Image and Font handles, Renderer trait. `syrp-peer-api`: window peer, input events, IME, clipboard, cursors, displays, drag and drop, native dialogs. |
| **L2 Core** | `syrp-strings` (LocalisedStrings, TextDiff, Base64, StringPool). `syrp-maths` (BigInteger, Expression, Range, NormalisableRange, Random, statistics). `syrp-sys` (SystemStats, Time, Uuid, Logger, registry, permissions). `syrp-var` (`Var`, DynamicObject, NamedValueSet, Identifier, JSON, XmlElement). `syrp-io` (streams, File, MemoryBlock, mmap, zip/gzip, MIME). `syrp-thread` (Thread, ThreadPool, TimeSliceThread, HighResolutionTimer, WaitableEvent). `syrp-process` (ChildProcess, InterProcessLock, DynamicLibrary). `syrp-net` (URL, WebInputStream, sockets, IP and MAC addresses, NamedPipe). Umbrella: `syrp-core`. |
| **L2 Events and state** | `syrp-events`: desktop MessageLoop implementations, ManualLoop, Timer, AsyncUpdater, broadcasters. `syrp-ipc`: InterprocessConnection, child-process coordinator, service discovery. `syrp-state`: ValueTree, Value, CachedValue, UndoManager, PropertiesFile, synchroniser, compat formats. |
| **L2 Headless extras** | `syrp-crypto`, `syrp-osc`, `syrp-analytics`, `syrp-unlock`, `syrp-js`. |
| **L3 Audio** | `syrp-audio-basics`: AudioBuffer, vector ops, SmoothedValue, ADSR, sources, Synthesiser, AudioWorkgroup. `syrp-mpe`. `syrp-midi-ci`. `syrp-dsp-{core,filters,fx,conv}` plus umbrella `syrp-dsp`. `syrp-formats`: manager, sources, sampler, buffering. `syrp-format-{wav,aiff,flac,ogg,mp3,opus,platform}`. `syrp-devices`: AudioDeviceManager, transport source, player. `syrp-audio-{coreaudio,wasapi,asio,alsa,jack,pipewire,aaudio,ios,web}`. `syrp-midi-io` plus `syrp-midi-{coremidi,windows,alsa,android,web}`. `syrp-processors`: processor runtime, ParamTree, ProcessorGraph, scanning, KnownPluginList. `syrp-host-{clap,vst3,au,lv2,ladspa,ara}`. |
| **L3 Graphics** | `syrp-gfx`: Graphics front-end, path ops, colour, gradients, effects. `syrp-image`: Image, codecs, cache, convolution. `syrp-text`: fonts, fallback, shaping, layout, AttributedString, GlyphArrangement. `syrp-drawable`: Drawable tree and SVG. `syrp-render-cpu` (vello_cpu). `syrp-render-gpu` (vello_gpu on wgpu). |
| **L3 Peers** | `syrp-peer-{win32,appkit,x11,wayland,uikit,android,web}`. `syrp-a11y`: bridge to the AccessKit adapters. |
| **L4 UI** | `syrp-ui`: component tree, events, focus, painting, modal, Desktop. `syrp-ui-layout`: FlexBox, Grid, stretchable, constrainers. `syrp-commands`. `syrp-laf`: LookAndFeel framework plus V1–V4 themes. `syrp-widgets-{buttons,slider,text,lists,menus,panels}`. `syrp-windows`. `syrp-filebrowser`. `syrp-app`. `syrp-animation`. `syrp-opengl`. |
| **L5 UI extras and audio UI** | `syrp-ui-extra`, `syrp-code-editor`, `syrp-webview`, `syrp-embed`, `syrp-audio-utils`, `syrp-processors-ui`, `syrp-unlock-ui`, `syrp-physics2d`, `syrp-video`. |
| **L5 Plugins** | `syrp-plugin`: format-neutral wrapper runtime and `export_plugin!`. `syrp-wrap-{clap,vst3,auv2,auv3,lv2,aax,unity,standalone,ara}`. `syrp-bundler`: bundles, plists, moduleinfo.json, LV2 TTL, signing hooks. |
| **L6 Tooling** | `syrp-testkit`: offline render, ManualLoop harness, golden comparators, fake host, UI driver. `syrp-inspect`: JSON dumps. `syrp-parity`: ledger schema and tools. `syrp-docgen`: API index, llms.txt, briefs, migration map. `syrp-resources`: binary-data `build.rs` helper. `syrp-cli`: the `cargo syrp` binary. `syrp-mcp`: MCP server. |
| **L6 Facade and apps** | `syrp`: feature-gated umbrella. `apps/host-app` (AudioPluginHost). `apps/demos` (DemoRunner). `apps/perf`. |
| **L7 Interop** (Phase 8) | `syrp-capi`, `syrp-cxx`, `syrp-py`, `syrp-uniffi`, `syrp-wasm`, `syrp-dotnet`. |

### 5.3 Layered dependency list
Arrows read "depends on". Third-party crates are shown in brackets. No crate may depend on a crate in a higher layer, and siblings within a group never depend on each other. `xtask lint-layers` enforces both rules.

```text
// L0
syrp-error      -> (none)
syrp-rt         -> error [rtrb]
syrp-geom       -> (none) [kurbo]
// L1 contracts
syrp-loop-api   -> error
syrp-audio-api  -> error, rt
syrp-midi       -> error, rt
syrp-format-api -> audio-api
syrp-device-api -> audio-api, midi, loop-api
syrp-plugin-api -> audio-api, midi, rt
syrp-gfx-api    -> error, geom [peniko]
syrp-peer-api   -> geom, gfx-api, loop-api [raw-window-handle, accesskit, keyboard-types, cursor-icon]
// L2
syrp-strings, syrp-maths, syrp-sys -> error
syrp-var        -> error, strings [serde, serde_json, quick-xml]
syrp-io         -> error, sys [flate2, zip]
syrp-thread     -> error, rt, sys
syrp-process    -> thread, io
syrp-net        -> io, thread [ureq, rustls]
syrp-core       -> var, io, thread, process, net, maths, sys, strings      (umbrella)
syrp-events     -> loop-api, thread
syrp-ipc        -> events, net, process [interprocess]
syrp-state      -> var, events, io [rpds]
syrp-crypto     -> maths, io [md-5, sha2, blowfish, whirlpool, num-bigint]
syrp-osc        -> net, events [rosc]
syrp-analytics  -> net, events
syrp-unlock     -> crypto, net, var
syrp-js         -> var [rquickjs]
// L3 audio
syrp-audio-basics -> audio-api, rt, maths
syrp-mpe          -> midi, audio-basics
syrp-midi-ci      -> midi
syrp-dsp-core     -> audio-basics [rustfft, realfft]
syrp-dsp-{filters,fx,conv} -> dsp-core            ; syrp-dsp -> all dsp-*
syrp-format-{wav,aiff,flac,ogg,mp3,opus,platform} -> format-api, io   (parallel leaves)
syrp-formats      -> format-api, audio-basics, io
syrp-audio-{coreaudio,wasapi,asio,alsa,jack,pipewire,aaudio,ios,web} -> device-api, rt   (parallel leaves)
syrp-midi-{coremidi,windows,alsa,android,web} -> device-api, midi                          (parallel leaves)
syrp-devices      -> device-api, audio-basics, events
syrp-midi-io      -> device-api, midi, events
syrp-processors   -> plugin-api, audio-basics, midi, state
syrp-host-{clap,vst3,au,lv2,ladspa,ara} -> processors, plugin-api   (parallel leaves)
// L3 graphics and peers
syrp-gfx          -> gfx-api, geom
syrp-image        -> gfx-api, io [image, png, zune-jpeg, image-webp]
syrp-text         -> gfx-api, io [parley, fontique, harfrust, skrifa, icu_segmenter]
syrp-drawable     -> gfx, image, text [usvg]
syrp-render-cpu   -> gfx-api [vello_cpu]
syrp-render-gpu   -> gfx-api, peer-api [vello_gpu, wgpu]
syrp-a11y         -> peer-api [accesskit_*]
syrp-peer-{win32,appkit,x11,wayland,uikit,android,web} -> peer-api, loop-api, a11y   (parallel leaves)
// L4 UI
syrp-ui           -> gfx, text, image, peer-api, events, state, a11y
syrp-ui-layout, syrp-commands -> ui [taffy]
syrp-laf          -> ui, gfx, text, drawable
syrp-widgets-{buttons,slider,text,lists,menus,panels} -> ui, laf, ui-layout   (parallel leaves)
syrp-windows      -> ui, laf, widgets-buttons, widgets-text
syrp-filebrowser  -> windows, widgets-lists, io
syrp-app          -> ui, windows, events
syrp-animation    -> ui, events
syrp-opengl       -> ui, peer-api [glow]
// L5
syrp-ui-extra     -> windows, widgets-*
syrp-code-editor  -> widgets-text
syrp-webview      -> ui, peer-api, var, ipc [wry, webview2-com, webkit2gtk]
syrp-embed        -> ui, peer-api
syrp-processors-ui-> processors, windows, widgets-*
syrp-audio-utils  -> ui, widgets-*, devices, formats, processors
syrp-unlock-ui    -> unlock, windows
syrp-physics2d    -> gfx [rapier2d]
syrp-video        -> ui, peer-api
syrp-plugin       -> plugin-api, processors, (feature "editor": ui, peer-api)
syrp-wrap-{clap,vst3,auv2,auv3,lv2,aax,unity,standalone,ara} -> plugin   (parallel leaves)
                       [clack-plugin | vst3 | objc2-audio-toolbox | lv2 headers | cxx+AAX SDK | ARA headers]
syrp-wrap-standalone -> plugin, devices, midi-io, windows
syrp-bundler      -> io, var
// L6
syrp-testkit      -> events, processors, render-cpu, ui, a11y, formats
syrp-inspect      -> ui, processors, state, devices
syrp-parity, syrp-docgen -> var, io [rustdoc-types, schemars]
syrp-cli          -> testkit, inspect, parity, docgen, bundler [clap]
syrp-mcp          -> cli (library API) [rmcp]
syrp              -> (all, feature-gated)
```

### 5.4 Threading and real-time model
**Thread roles**

| Role | What runs there |
|---|---|
| **Message thread** | UI, ValueTree, Component, timers. In apps this is the main thread. In plugins it is the host's UI thread. |
| **Audio threads** | Device callbacks and host `process` calls. There may be several, for example with CLAP thread pools or graph parallelism. |
| **Background workers** | ThreadPool and TimeSliceThread: file I/O, plugin scanning (out of process), graph compilation, thumbnail building. |
| **Render threads** | Optional GPU and OpenGL rendering. |

**Enforcement by types**
- `MessageThread<'a>` is a zero-sized, `!Send`, `!Sync` token. It exists only inside message-loop callbacks or through `MessageLoop::with_token()`. Every UI and ValueTree mutation takes `&MessageThread`.
- `RtCtx<'a>` is created only by the framework around `process`. It exposes RT-safe services: a lock-free logger, a deferred-drop handle, transport, and parameter-change iterators. `PrepareCtx` is the only context allowed to allocate audio-path memory.
- Types usable from `process` implement `RtSafe`. It is an `unsafe` marker trait, and claiming it puts a PR in the high-risk review category.

**Communication between threads**
- Parameters: atomics (`AtomicF32` with sample-accurate change queues).
- UI → audio: rtrb SPSC queues.
- Audio → UI: `TripleBuffer` for meters and scopes.
- Configuration swaps: an `RtHandoff<T>`, which publishes atomically and reclaims the old value through the deferred-drop collector, never on the audio thread.

**Runtime verification**
- The debug allocator guard (forked from assert_no_alloc) aborts with `SYRP-RT-0001` on any allocation inside `RtCtx` scopes.
- An RTSan CI job uses rtsan-standalone, plus nightly rustc RTSan once available.
- loom models cover every lock-free primitive.

```rust
pub struct MessageThread<'a> { _p: PhantomData<(&'a (), *const ())> } // !Send + !Sync
pub struct RtCtx<'a> { /* transport, rt_log, defer: DeferredDrop<'a>, ... */ }
pub unsafe trait RtSafe {}

pub trait Processor: Send + 'static {
    type Params: ParamSet;                       // generated or hand-written; RT-readable atomics
    fn prepare(&mut self, cfg: &ProcessConfig, cx: &mut PrepareCtx) -> Result<(), PrepareError>;
    fn process(&mut self, io: &mut ProcessIo<'_>, p: &Self::Params, rt: &mut RtCtx<'_>);
    fn reset(&mut self) {}
}
// Does not compile: no allocation API is reachable from ProcessIo/RtCtx,
// and `PrepareCtx` cannot be obtained inside `process` (trybuild-tested).
```

### 5.5 Event loop and message-thread design
`MessageLoop` (from `syrp-loop-api`) is a trait with three kinds of implementation:

1. **Owned loops** for apps:
   - Win32: a hidden message window.
   - macOS and iOS: CFRunLoop and NSApplication or UIApplication.
   - Linux: an epoll loop over X11/Wayland file descriptors.
   - Android: a Looper.
   - Web: the browser task queue.
2. **Hosted loops** for plugins. The framework attaches to the host:
   - CLAP: `timer-support` and `posix-fd-support`.
   - VST3 on Linux: `IRunLoop`.
   - AU and AAX: the platform main run loop.
   - JUCE's lazy-initialisation behaviour is reproduced, but explicitly.
3. **`ManualLoop`** for tests. It has a virtual clock: `advance(Duration)` and `run_until_idle()`. This makes every timer, animation and async update deterministic, which is the basis of A9 and A10.

JUCE features map as follows:

| JUCE | Syrp |
|---|---|
| `callAsync` | `post(task)` |
| `callSync` | `post_and_wait` (forbidden on the message thread itself) |
| Timer, MultiTimer | `timer(period)` returning a guard |
| AsyncUpdater | `Coalescer` |
| ChangeBroadcaster | `Broadcaster<T>` |
| VBlankAttachment | `on_vblank` |
| MessageManagerLock | Not provided. Use `post_and_wait` or futures. This is a documented deviation, because the lock pattern is error-prone for agents. |

```rust
pub trait MessageLoop {
    fn post(&self, task: Task);                                   // from any thread
    fn timer(&self, period: Duration, f: TimerFn) -> TimerGuard;  // message thread
    fn on_vblank(&self, peer: PeerId, f: VBlankFn) -> VBlankGuard;
}
let mut lp = ManualLoop::new();                  // tests: deterministic
let _t = lp.timer(Duration::from_millis(30), |mt| { /* ... */ });
lp.advance(Duration::from_millis(100));          // fires exactly 3 times
```

### 5.6 Component and UI model
**Retained tree in an arena.**
- `Ui` owns a slotmap of nodes keyed by `ComponentId`. Each node holds bounds, transform, visibility, z-order, opacity, cached-image policy, focus flags, accessibility info and the boxed behaviour object.
- User types implement `Component`, with JUCE-familiar hooks given default implementations.
- Parent and child links live in the arena, not in user structs. This avoids `Rc` cycles and makes the tree trivially serialisable for `inspect`.

**Typed handles.** `Handle<T: Component>` gives checked downcasting: `ui.get_mut(slider_handle)`.

**Events.**
- The mouse, touch and pen pipeline mirrors JUCE's MouseInputSource semantics: enter, exit, drag thresholds, double-click timing and capture.
- Keyboard focus covers traversal order, focus containers and modal stacks. Commands are handled by `syrp-commands`.
- IME goes through `TextInputTarget`.

**Painting.**
- Dirty regions are coalesced per peer and painted on VBlank.
- `paint` receives `&mut Graphics` (§5.7). `paint_over_children` is supported.
- Per-component cached layers replace CachedComponentImage.

**LookAndFeel.**
- A trait with one method per drawable widget part, such as `draw_rotary_slider` and `draw_button_background`, plus colour IDs as typed theme tokens.
- V1–V4 are provided as implementations. LookAndFeel is resolved up the tree, as in JUCE.

**Layout.** `resized` hooks, plus `syrp-ui-layout`: FlexBox and Grid via taffy, StretchableLayout and constrainers.

**Accessibility.** Each node contributes an AccessKit node through `accessibility()`. Defaults come from the widget's role. The same tree drives screen readers, `inspect` and test scripting.

**Optional declarative layer.** A serde-describable `UiSpec` (JSON/TOML) builds standard widget trees and parameter bindings. It is the format agents and WebView relays use, and it compiles to ordinary component calls.

```rust
pub trait Component: 'static {
    fn paint(&mut self, g: &mut Graphics, cx: &PaintCx) {}
    fn resized(&mut self, cx: &mut LayoutCx) {}
    fn mouse_down(&mut self, e: &MouseEvent, cx: &mut EventCx) -> Handled { Handled::No }
    fn key_pressed(&mut self, k: &KeyPress, cx: &mut EventCx) -> Handled { Handled::No }
    fn accessibility(&self, node: &mut a11y::NodeBuilder) {}
}
let root   = ui.set_root(mt, Panel::new());
let gain   = ui.add_child(mt, root, Slider::rotary().range(-60.0..=12.0).suffix(" dB"));
ui.attach_param(mt, gain, params.gain());          // ParameterAttachment equivalent
ui.set_bounds(mt, gain, Rect::new(10, 10, 80, 80));
let png = testkit::render(&ui, Scale(2.0))?;        // headless golden
```

### 5.7 Graphics abstraction
**`Graphics` front-end** (in `syrp-gfx`). JUCE-like immediate calls are recorded into a `DisplayList` defined in `syrp-gfx-api`. The calls include:
- `fill_rect`, `fill_path`, `stroke_path`;
- `draw_image` with RectanglePlacement and resampling quality;
- `draw_text` and `draw_glyphs` (glyph runs from `syrp-text`);
- `reduce_clip` (rect, path, image mask), `add_transform`;
- `begin_transparency_layer`, blend modes, and gradients (linear, radial, two-point conical).

**Renderers** implement `trait Renderer { fn render(&mut self, list: &DisplayList, target: &mut RenderTarget) -> Result<(), RenderError> }`.

| Renderer | Built on | Role |
|---|---|---|
| `syrp-render-cpu` | vello_cpu | The reference renderer. Deterministic mode for goldens, software fallback, headless. |
| `syrp-render-gpu` | vello_gpu on wgpu: Metal, D3D12, Vulkan, GLES, WebGPU, WebGL2 | The default for windows once validated per platform. |

Optional native backends (Direct2D, CoreGraphics) are deferred. They would only be added if parity testing shows that text or AA fidelity requires them (§11).

**Images.** `Image` is backed by CPU pixels with lazy GPU upload. `ImageCache`, convolution kernels and effects (DropShadow, Glow) are implemented as DisplayList filters.

**Text.** `syrp-text` wraps parley, fontique and harfrust and provides JUCE's Font/FontOptions, FontFeatures, variable-font axes, AttributedString, TextLayout, GlyphArrangement and justified text. Glyph outlines come from skrifa, so the CPU and GPU renderers draw identical outlines.

**Scale factors.** Logical-to-physical scaling is explicit in `RenderTarget`. Golden tests run at 1x, 1.25x, 1.5x and 2x.

### 5.8 Audio processor, graph and plugin-wrapper model
**Processor.** The Processor trait was sketched in §5.4. It is complemented by:
- `BusLayout` negotiation, using `ChannelSet` (mono through Atmos-style layouts);
- latency and tail reporting;
- precision (f32 and f64);
- bypass;
- MIDI and UMP event I/O (`EventIo`);
- `save_state` and `load_state`.

**Parameters.**
- A `ParamSpec` is declared by builder, by TOML, or by an optional derive. Each parameter has an ID, name, range (NormalisableRange semantics), default, unit, text conversion, smoothing, automation flags and group.
- At runtime, `Params` provides atomics and sample-accurate change iterators.
- `ParamTree` in `syrp-processors` is the AudioProcessorValueTreeState equivalent: atomics plus a ValueTree mirror for persistence and undo.

**Editor.** `trait Editor { fn create(&mut self, ui: &mut Ui, params: &ParamHandles, mt: MessageThread) -> ComponentId; fn constraints(&self) -> SizeConstraints; }`. It is created and destroyed on the message thread and never touches the audio state directly.

**ProcessorGraph.**
- Editing (`add`, `connect`, `remove`) happens on the message thread.
- `compile()` runs on a worker. It topologically sorts nodes, allocates buffers, inserts latency-compensation delays, and resolves MIDI routing into a `RenderPlan`.
- The plan is published to the audio thread through `RtHandoff`, and old plans are reclaimed through deferred drop.
- Graph I/O nodes mirror JUCE's AudioGraphIOProcessor.

**Wrapper runtime** (`syrp-plugin`).
- A format-neutral `PluginInstance` handles:
  - parameter bridging (host ↔ atomics ↔ UI), with gesture begin and end;
  - state chunks, with versioned envelopes and an optional reader for JUCE-compatible blobs;
  - bus negotiation;
  - transport and PlayHead mapping;
  - the editor lifecycle in host-provided parent windows (via `syrp-peer-*` child mode);
  - scale-factor changes;
  - host-quirk tables, recorded as data in the ledger.
- Each `syrp-wrap-*` translates one host API to `PluginInstance`.
- `export_plugin!(MyPlugin)` is the single sanctioned macro, because FFI entry symbols must be generated. It is fully documented, with an `expand` snapshot.

**Hosting.** `syrp-host-*` implement a `HostedPlugin` that is itself a `Processor`. Hosted plugins can therefore sit in a ProcessorGraph, as in AudioPluginHost. Scanning runs out of process, as JUCE's AudioPluginHost can be configured to do, so a crashing plugin cannot take the host down.

```rust
let mut g = ProcessorGraph::new(mt);
let synth = g.add(mt, host_clap::load(&desc)?);       // hosted CLAP plugin
let gain  = g.add(mt, Gain::new());
g.connect(mt, synth.audio_out(0..2), gain.audio_in(0..2))?;
g.connect(mt, gain.audio_out(0..2),  g.output().audio_in(0..2))?;
let plan_ticket = g.compile_async(mt);                 // swapped into RT when ready
```

### 5.9 ValueTree, undo and state model
**`ValueTree`.**
- Internally an `Rc`-shared node with a type `Identifier`, ordered properties (`Identifier → Var`), children and listeners.
- Message-thread only: every mutation takes `&MessageThread`, and the type is `!Send`.
- Cloning is shallow, as in JUCE, where `ValueTree` is a reference. `deep_clone()` is explicit.
- Listener callbacks are queued and delivered after the mutation, so they cannot re-enter it. The ordering rules are documented and tested against the harness.

**`UndoManager`.**
- Transactions with names, coalescing, `begin_new_transaction`, and per-transaction undo and redo.
- `UndoableAction` is a trait.
- ValueTree mutations accept `Option<&mut UndoManager>`.

**`Value` and `CachedValue<T>`.** Observable cells, bridged to ValueTree properties.

**Persistence.**
- XML (quick-xml), JSON and a binary format.
- The **`juce-compat` feature** adds readers and writers for JUCE's ValueTree binary and XML layouts. They are specified by black-box observation of harness-produced files (§10). This is how ported plugins keep their sessions.
- `PropertiesFile` and `ApplicationProperties` use platform-appropriate locations.

**`ValueTreeSynchroniser`.** Emits change deltas. The same delta stream feeds `inspect`, WebView UIs (JSON patches) and remote editors.

**Typed views.** An optional `#[derive(StateSchema)]`, or hand-written equivalents, provides typed accessors over a ValueTree branch, so agents get compile-time property names.

```rust
let tree = ValueTree::new(mt, id("Synth"));
let mut undo = UndoManager::new();
tree.set_property(mt, id("cutoff"), Var::from(1200.0), Some(&mut undo))?;
tree.on_property_changed(mt, |ev| tracing::info!(prop = %ev.property, "changed"));
undo.undo(mt)?;
let bytes = tree.to_binary(BinaryFormat::Syrp)?;        // or BinaryFormat::JuceCompat
```

## 6. Platform backends

The table below gives the chosen backend for each subsystem on each platform. In the audio and MIDI rows, the cpal and midir wrappers deliver an early version. Native backends then replace them where parity requires (§4.1).

| Subsystem | Windows | macOS | Linux | iOS | Android | Web / wasm |
|---|---|---|---|---|---|---|
| **Audio** | WASAPI shared, exclusive and low-latency (`IAudioClient3`) via `windows`/`wasapi`; ASIO with a user-supplied SDK (GPLv3 or proprietary); DirectSound as a deliberate gap | CoreAudio HAL with aggregate devices (JUCE 9 parity) and AudioWorkgroup via objc2-core-audio | ALSA, JACK and PipeWire (PulseAudio through PipeWire) | AVAudioSession plus RemoteIO (objc2), with interruption and route handling | AAudio via `ndk`; OpenSL ES is a gap below API 26 | AudioWorklet (threads plus SharedArrayBuffer), with a WebAudio fallback (cpal backends) |
| **MIDI** | WinMM for MIDI 1.0, plus **Windows MIDI Services** (UMP, multi-client) through a WinRT projection from `windows` | CoreMIDI with UMP protocol | ALSA sequencer, ALSA UMP rawmidi (kernel ≥ 6.5, UNVERIFIED minimum), JACK MIDI | CoreMIDI plus BLE MIDI pairing UI | `android.media.midi` via AMidi (NDK) and JNI; BLE pairing | Web MIDI API (web-sys) |
| **Windowing / peer** | Win32 (`windows`), child HWND for plugins, per-monitor DPI v2 | AppKit NSView (objc2), child NSView for plugins | X11 (x11rb) for plugins and apps, XEmbed for embedding; Wayland (SCTK) for standalone apps only | UIKit UIView (objc2) and the UIScene lifecycle | android-activity (GameActivity) plus JNI View for embedding | `<canvas>` via web-sys and winit's web backend |
| **Graphics** | vello_gpu on wgpu D3D12 (Vulkan fallback); vello_cpu fallback | vello_gpu on wgpu Metal; vello_cpu | vello_gpu on wgpu Vulkan or GL; vello_cpu | vello_gpu on Metal | vello_gpu on Vulkan or GLES | vello_gpu on WebGPU or WebGL2; vello_cpu into a canvas ImageData |
| **Fonts** | fontique (DirectWrite enumeration) + harfrust + skrifa | fontique (CoreText enumeration) | fontique (fontconfig) | fontique (CoreText) | fontique (Android system fonts) | Bundled fonts only; a manifest is required |
| **WebView** | WebView2 via wry and webview2-com | WKWebView via wry | WebKitGTK via wry; for plugins, an **out-of-process helper**, as JUCE does | WKWebView via wry | Android WebView via wry | Native DOM `<iframe>` / overlay (an extension beyond parity) |
| **Plugin formats** (built with Syrp) | CLAP, VST3, AAX (shim), Unity, Standalone; VST2 is a gap | CLAP, VST3, AUv2, AUv3, AAX, Unity, Standalone | CLAP, VST3, LV2, Unity (UNVERIFIED: does JUCE support Unity on Linux?), Standalone | AUv3 (app extension), Standalone | Standalone only. There is no Android plugin standard, and JUCE has none either. | Standalone wasm app. Web Audio Modules would be an optional extension (UNVERIFIED maturity). |
| **Plugin hosting** | CLAP, VST3, ARA | CLAP, VST3, AU, ARA | CLAP, VST3, LV2, LADSPA | AUv3 | none | none |
| **Accessibility** | AccessKit UI Automation | AccessKit NSAccessibility | AccessKit AT-SPI (zbus) | AccessKit UIAccessibility | AccessKit Android | DOM/ARIA mirror (the AccessKit web adapter is planned upstream) |

**Proposed minimum OS targets.** These are user decisions (§11).

| Platform | Proposed minimum | JUCE 9 minimum | Reason for the difference |
|---|---|---|---|
| Windows | 10 1607 | 10 1607 | Same as JUCE. |
| macOS | 11 | 10.11 | Apple Silicon, AudioWorkgroup and Rust target support. Rust's support for `x86_64-apple-darwin` is UNVERIFIED for 2026. |
| iOS | 13 | 12 | UIScene. |
| Android | API 26 | API 24 | AAudio. |
| Linux | glibc 2.31 class distros, X11 and Wayland | mainstream distros | |
| Web | Browsers with AudioWorklet, SharedArrayBuffer (COOP/COEP) and WebGL2 | not supported | Web goes beyond JUCE parity. |

Each target that differs from JUCE is a ledger "deviation" row.

## 7. Phased roadmap

### 7.1 Throughput model and assumptions
**Team.**
- On average 12 implementer agent slots (range 10–15), 3 reviewer agents (one of them in the adversarial pool), 1 PR-manager agent and 1–2 spec-room agents.
- A deterministic orchestrator schedules the work (§12).
- Humans contribute about 0.5 FTE in total: escalations, legal, device-lab upkeep, signing certificates, DAW licences.

**Work package (WP).**
- One crate or sub-module.
- 1.5k–6k lines including tests.
- Delivered as 1–4 PRs of at most about 1,200 changed lines each.
- Every WP has a machine-readable spec, frozen contracts, parity scenarios and an acceptance command (§12).

**Speed assumptions.** These are estimates, not measurements.

| Metric | Assumption |
|---|---|
| Time to produce one PR-sized increment | 4–10 agent-hours |
| Review rounds per PR | 1.8 on average |
| Reviewer turnaround | 1–3 hours per round |
| Full CI matrix | 60–120 minutes |
| WP lead time, typical | 2–6 working days |
| WP lead time, hard (wrappers, device backends, text, TextEditor, CoreAudio) | 2–4 weeks, including parity rework and hardware or DAW testing |

**Effective throughput.**
- About **6–9 merged WPs per week**, not 12+. Rework, the device-lab queue, and serialisation through shared files and contract changes all slow things down.
- There are an estimated 240–300 WPs in Phases 0–7.

**Waves.**
- A wave has 10–15 WPs that are independent under the §5.3 dependency graph.
- Waves **roll**: a WP from wave *n+1* starts as soon as its own dependencies have merged, not when the whole wave finishes.
- Each phase ends with a **parity sweep**, made of ledger-gap WPs.

**Agent acceleration.**
- Compared with a 10-person senior human team, agents are about 3–4× faster on well-specified leaf crates.
- About 1.5× on platform and plugin integration, which is bounded by hardware and host testing.
- About 1× on legal, signing and store processes.

**Result.** About **16–20 months to Rust feature parity** (Phases 0–7) and about **3–4 more months for interop** (Phase 8).

**Critical path:** contracts → `syrp-ui` core → widgets → plugin editors → AU/VST3 host validation.

**Why the phases are longer than throughput alone suggests.** About 233 WPs are listed below. At 6–9 per week, that is only 26–39 weeks of pure throughput. The phases add up to about 76 weeks for three reasons:
- **Waves inside a phase form chains.** For example, 2.1 → 2.2 → 2.3, and 3.1 → 3.2 → 3.3.
- **The hardest WP in each chain sets the pace.** Hard WPs take 2–4 weeks each.
- **Hardware and DAW validation queue serially.**

The binding constraint is therefore the critical path, not agent count. Adding more than 15 implementers would mostly add idle slots.

### 7.2 Phases

**Phase 0: Foundations, contracts and closed-loop tooling (≈6 weeks).** All later agents depend on the tooling, so it lands first.

**Wave 0.1, bootstrap (12 WPs):**
1. Repo scaffolding: workspace, toolchain pin, lints, `deny.toml`, AGENTS.md/CLAUDE.md, PR and issue templates, `ownership.toml`. Done by the PR manager.
2. CI skeleton: Linux, macOS and Windows; fmt, clippy, nextest, doc, deny.
3. `syrp-error` and the error-catalogue generator.
4. `syrp-rt`.
5. `syrp-geom`.
6. Parity ledger schema plus a seeded inventory of every JUCE 9.0.4 class from public docs (spec room).
7. WP spec schema plus orchestrator bootstrap: task board sync, claim and lease, agent spawning.
8. `cargo syrp` skeleton: JSON envelope, exit codes, `doctor`, `explain`, `wp`.
9. `syrp-testkit` v0: offline audio render and numeric comparators.
10. Docs pipeline v0: mdBook, rustdoc JSON → `api-index.json`, llms.txt.
11. `juce-oracle` differential harness, a separate AGPL repository (§9).
12. Clean-room contamination scanner (quarantined job) and provenance headers.

**Wave 0.2, contracts and the loop (13 WPs):**
1. `syrp-loop-api` plus `ManualLoop`.
2. `syrp-audio-api`.
3. `syrp-midi` (types and UMP).
4. `syrp-format-api`.
5. `syrp-device-api`.
6. `syrp-plugin-api`.
7. `syrp-gfx-api`.
8. `syrp-peer-api`.
9. Renderer spike: vello_cpu deterministic mode vs tiny-skia, and the golden comparator.
10. `syrp-mcp` v0.
11. Agent Skill v0.
12. Docs generator v1 (crate briefs, migration-map scaffold, error catalogue pages).
13. Agent-eval harness v0 (10 tasks).

**Exit criteria (testable):**
- All 8 contract crates are frozen as `contracts/*.public-api.txt`.
- `cargo syrp test`, `parity`, `explain` and `docs` run with JSON output that validates against the published schemas.
- An implementer agent completes a toy WP end to end through CLI, MCP, the reviewer and the PR manager with no human input.
- The ledger covers 100% of the JUCE 9.0.4 public classes.

**Phase 1: Core runtime (≈6–7 weeks).** Covers juce_core, juce_events, juce_data_structures, juce_cryptography and juce_osc.

**Wave 1.1 (14 WPs):**
1. `syrp-var`: Var, DynamicObject, Identifier.
2. JSON and XML DOM (also in `syrp-var`).
3. `syrp-io` streams and MemoryBlock.
4. File, special locations, directory iteration and mmap.
5. zip and gzip.
6. `syrp-thread`.
7. HighResolutionTimer and TimeSliceThread.
8. `syrp-process`.
9. `syrp-net`: URL and WebInputStream.
10. Sockets, IP and MAC addresses, NamedPipe.
11. `syrp-maths`.
12. `syrp-sys`.
13. `syrp-strings`.
14. Desktop owned MessageLoop implementations (Win32, CFRunLoop, epoll).

**Wave 1.2 (12 WPs):**
1. Timers, Coalescer and broadcasters.
2. `syrp-ipc`.
3. ValueTree core.
4. UndoManager.
5. Value and CachedValue.
6. PropertiesFile and ApplicationProperties.
7. ValueTreeSynchroniser.
8. `juce-compat` state formats (black-box spec).
9. `syrp-crypto`.
10. `syrp-osc`.
11. Headless analytics.
12. Key-file generation in `syrp-unlock`.

**Exit criteria:**
- Differential suites pass: String/JSON/XML edge cases, BigInteger, Expression, ValueTree binary and XML round-trips against the harness, and the OSC wire format.
- Miri is clean on core crates, and loom models of the lock-free primitives pass.
- Ledger rows for these modules are `done`, `mapped` or approved `gap`.
- Agent-eval success is at least 70% on core tasks.

**Phase 2: Headless audio engine, desktop I/O and hosting (≈10–12 weeks).**

**Wave 2.1 (15 WPs):**
1. AudioBuffer and vector ops (SIMD).
2. Data converters and ChannelSet.
3. SmoothedValue, ADSR, Decibels and interpolators.
4. The AudioSource family.
5. Synthesiser.
6. `syrp-mpe`.
7. MidiMessage, MidiBuffer and MidiMessageSequence.
8. MidiFile.
9. UMP conversion and dispatcher.
10. `dsp-core`: FFT, windowing, SIMD, lookup tables.
11. `dsp-filters`.
12. `dsp-fx`.
13. `dsp-conv`: Convolution and Oversampling.
14. `format-wav` and `format-aiff`.
15. `format-flac`.

**Wave 2.2 (14 WPs):**
1. `format-ogg`.
2. `format-mp3`.
3. `format-opus`.
4. `format-platform`.
5. `syrp-formats`: manager, sources, sampler.
6. `syrp-devices`: manager, transport, player.
7. CoreAudio backend.
8. WASAPI backend.
9. ASIO backend.
10. ALSA backend.
11. JACK and PipeWire backends.
12. CoreMIDI.
13. Windows MIDI (WinMM and Windows MIDI Services).
14. ALSA MIDI (sequencer and UMP).

**Wave 2.3 (12 WPs):**
1. `syrp-midi-ci`.
2. Processor runtime and params.
3. ParamTree (AudioProcessorValueTreeState).
4. ProcessorGraph.
5. PluginDescription, KnownPluginList, out-of-process scanner.
6. `host-clap`.
7. `host-vst3`.
8. `host-au`.
9. `host-lv2` and `host-ladspa`.
10. `host-ara`.
11. AudioWorkgroup and RT priority.
12. `cargo syrp render-audio` / `inspect params|graph` / `host` (headless).

**Exit criteria:**
- DSP and codec differential suites pass within the §9 tolerances.
- Device backends pass loopback tests on device-lab hardware (latency is reported within ±1 buffer of a measured value).
- Hosting loads a reference plugin corpus (open-source CLAP, VST3, AU and LV2 plugins).
- The RTSan job and the allocation guard are clean on every audio path.

**Phase 3: Graphics, peers and GUI basics (≈12–14 weeks).**

**Wave 3.1 (13 WPs):**
1. `syrp-gfx` paths, strokes, clipping and transforms.
2. Colours, gradients and fill types.
3. `syrp-image` with codecs and effects.
4. `syrp-text` fonts and fallback.
5. `syrp-text` shaping and layout.
6. AttributedString, TextLayout and GlyphArrangement.
7. `render-cpu`.
8. `render-gpu`.
9. `syrp-drawable` and SVG.
10. `peer-win32`.
11. `peer-appkit`.
12. `peer-x11`.
13. `syrp-a11y`.

**Wave 3.2 (15 WPs):**
1. `syrp-ui` component tree and painting.
2. Mouse, touch and pen dispatch.
3. Keyboard, focus and IME.
4. `ui-layout`.
5. `syrp-commands`.
6. `syrp-laf` with the V4 theme.
7. `widgets-buttons`.
8. `widgets-slider`.
9. `widgets-text` (Label, TextEditor).
10. `widgets-menus` (ComboBox, PopupMenu, MenuBar).
11. `widgets-lists` (ListBox, Table, TreeView).
12. `widgets-panels`.
13. `syrp-windows`.
14. `syrp-filebrowser` with native choosers.
15. `peer-wayland` (standalone apps).

**Wave 3.3 (12 WPs):**
1. Drag and drop: internal and external.
2. Desktop, Displays and scale factors.
3. `syrp-animation`.
4. `syrp-app` lifecycle.
5. ComponentAnimator and VBlank.
6. LookAndFeel V1–V3.
7. `syrp-opengl`.
8. `inspect tree|a11y` and `render-ui` in the CLI.
9. GUI demo ports (DemoRunner GUI set) as goldens.
10. Accessibility parity suite.
11. Text-editing behaviour suite.
12. HiDPI and fractional-scale suite.

**Exit criteria:**
- Ported GUI demos match JUCE goldens within tolerance at 1x, 1.5x and 2x.
- Shaping glyph-ID parity is 100% on the font corpus.
- Accessibility trees match JUCE's roles and actions for every widget.
- Scripted UI tests run headless on all three desktop OSes.

**Phase 4: Plugins (≈12–14 weeks).**

**Wave 4.1 (13 WPs):**
1. `syrp-plugin` runtime.
2. `wrap-clap`.
3. `wrap-vst3`.
4. `wrap-auv2`.
5. `wrap-lv2`.
6. `wrap-standalone`.
7. `syrp-bundler` (plists, moduleinfo.json, TTL, codesign and notarise hooks).
8. `syrp-processors-ui` (GenericEditor, attachments, PluginListComponent).
9. `syrp-audio-utils`.
10. `apps/host-app`.
11. `cargo syrp bundle|validate|sign`.
12. JUCE plugin-state import.
13. Host-quirk table framework.

**Wave 4.2 (11 WPs):**
1. `wrap-auv3` (macOS and iOS app extension).
2. `wrap-ara` plus the ARA host demo.
3. `wrap-unity`.
4. `wrap-aax` shim.
5. MIDI 2.0 and UMP plus note expressions in the plugin API.
6. Sample-accurate automation.
7. CLAP polyphonic modulation and remote controls (beyond JUCE).
8. Validator farm in CI.
9. DAW smoke matrix automation.
10. Ports of JUCE example plugins (Gain, NoiseGate, Arpeggiator, MultiOutSynth, Surround, DSPModule, WebView demo shell).
11. A VST2 decision WP. It is a no-op if the gap is approved.

**Exit criteria:**
- Ported example plugins pass pluginval at strictness 10, clap-validator, the Steinberg validator, `auval -strict`, lv2lint and aaxval (AAX in Pro Tools Developer).
- They load and automate in the DAW smoke matrix (§9).
- State saved by the JUCE-built twin loads in the Syrp build.

**Phase 5: GUI extras and the remaining modules (≈8–10 weeks).**

**Wave 5.1 (14 WPs):**
1. `syrp-webview` core with wry.
2. WebView relays and the resource provider.
3. The npm-compatible JS interop package.
4. Linux WebView subprocess for plugins.
5. `syrp-js`.
6. `syrp-code-editor` with tokenisers.
7. `syrp-ui-extra` part 1: ColourSelector, KeyMappingEditor, PreferencesPanel, RecentlyOpenedFilesList, SplashScreen, BubbleMessage.
8. System tray and local notifications.
9. `syrp-embed` (HWND, NSView, XEmbed).
10. `syrp-physics2d`.
11. `syrp-video` desktop.
12. Camera desktop.
13. `syrp-unlock` online flow plus `unlock-ui`.
14. Analytics destinations.

**Wave 5.2 (10 WPs):**
1. LiveConstantEditor equivalent.
2. AnimatedAppComponent and FileBasedDocument.
3. Optional egui-in-editor adapter.
4. `syrp-perf`.
5. Remaining demo ports.
6. Desktop push notifications.
7. ContentSharer (desktop).
8. A Box2D-demo-equivalent golden.
9. OSC and IPC demos.
10. Parity sweep.

**Exit criteria:** Every desktop ledger row is `done`, `mapped` or an approved `gap`, and the WebView plugin demo passes the validators on all three OSes.

**Phase 6: Mobile, web and completion (≈10–12 weeks).**

**Wave 6.1 (14 WPs):**
1. `peer-uikit`.
2. iOS audio session backend.
3. iOS MIDI and BLE.
4. `peer-android`.
5. AAudio backend.
6. Android MIDI and BLE.
7. `peer-web`.
8. Web audio (AudioWorklet).
9. Web MIDI.
10. Mobile file choosers and ContentSharer.
11. Mobile push notifications.
12. In-app purchases (StoreKit 2 and Play Billing).
13. Mobile accessibility.
14. `cargo syrp run --device` packaging (via cargo-mobile2 / cargo-ndk).

**Wave 6.2 (12 WPs):**
1. AUv3 on iOS end to end.
2. Mobile WebView.
3. Mobile video and camera.
4. Mobile OpenGL ES.
5. iOS and Android GUI goldens.
6. Web golden runner (headless browser).
7. Mobile performance budgets.
8. Mobile lifecycle suite.
9.–12. Parity sweep WPs.

**Exit criteria:** Mobile and web ledger rows close, and the mobile demo apps pass store-validation dry runs (UNVERIFIED tooling for automating this).

**Phase 7: Parity closure and 1.0 hardening (≈6–8 weeks).**

**Wave 7.1 (≤15 WPs):** The ledger's remaining gaps, ordered by user impact.

**Wave 7.2 (12 WPs):**
1. Fuzzing campaigns.
2. Performance parity versus JUCE (benchmarks within ±10% or better).
3. API review and semver freeze.
4. Docs completeness.
5. Migration-map completeness.
6. Codemods.
7. Security review (unsafe audit, `cargo-vet`).
8. Replace lilv with Rust.
9. Release automation dry runs.
10. Agent-eval push to ≥ 85%.
11. Long-run soak tests (72-hour audio).
12. A 1.0 RC.

**Exit criteria (Rust parity 1.0):**
- 100% of ledger rows are `done`, `mapped` or an approved `gap`.
- All validators are green.
- Agent-eval is at least 85%.
- Zero open P0 or P1 parity bugs.
- Public API is frozen under semver.

**Phase 8: Interop (≈12–16 weeks; it starts only after the Phase 7 exit, as required). See §8.**

**Wave 8.1 (12 WPs):**
1. `syrp-capi` surface design (handles and error codes).
2. C ABI core and events.
3. C ABI for audio and processors.
4. C ABI for UI hosting.
5. cbindgen pipeline and header tests.
6. `syrp-cxx` (C++ RAII wrapper over the C ABI).
7. C++ plugin-authoring path.
8. `syrp-py` (PyO3, numpy buffers, offline render).
9. maturin wheels CI.
10. UniFFI Swift and Kotlin.
11. wasm/JS (wasm-bindgen).
12. C# (csbindgen).

**Wave 8.2 (10 WPs):**
1. Binding-drift checks.
2. Cross-language golden tests.
3. JUCE-migration C++ guide and codemods.
4. Optional JUCE-shaped facade (only if §10 and §11 approve it).
5. Docs for each language.
6. Agent skills for each language.
7.–10. Packaging (vcpkg, Conan, PyPI, SwiftPM, Maven, NuGet, npm).

**Exit criteria:** Each binding passes the cross-language subset of the parity suite, and generated bindings show zero drift from the Rust API index.

**Tooling growth by phase.** The CLI, MCP server and skill gain commands as each phase lands. The full list is in §13.

| Phase | New tooling |
|---|---|
| 0 | `doctor`, `explain`, `test`, `parity`, `docs`, `wp` |
| 2 | `render-audio`, `inspect params\|graph`, `host` |
| 3 | `render-ui`, `snapshot`, `inspect tree\|a11y` |
| 4 | `new plugin`, `bundle`, `validate`, `sign` |
| 6 | `run --device`, `package` |
| 8 | `bindings` |

## 8. Interop phase

Phase 8 starts only after the Rust parity exit (Phase 7). Interop cannot distort the Rust API, because only a curated, stable subset is exported. Every binding is **generated** from one annotated source and verified against the API index.

### 8.1 Layering
```text
Rust API (syrp-*)  ->  syrp-capi (curated C ABI, handles + error codes)  ->  generated bindings
                                         |-> C header (cbindgen)
                                         |-> C++ RAII wrapper (generated; diplomat-style backend)
                                         |-> C# P/Invoke (csbindgen)
Rust API  ->  syrp-py (PyO3/maturin, numpy-aware)        [direct, not via C ABI]
Rust API  ->  syrp-uniffi (UniFFI -> Swift, Kotlin)       [app-level, non-RT]
Rust API  ->  syrp-wasm (wasm-bindgen -> JS/TS)           [web apps]
```

### 8.2 Per-language plan
| Target | Tool (version, licence) | Scope | Notes |
|---|---|---|---|
| **C ABI** | cbindgen 0.29 (MPL-2.0 tool; generated headers carry Syrp's licence) | Opaque handles (`syrp_ui_t*`, `syrp_processor_t*`), `syrp_status` error codes taken from the error catalogue, callbacks with `void* user`, explicit ownership functions, `SYRP_ABI_VERSION`, and an `syrp_` symbol prefix | Every exported function wraps `catch_unwind`. RT-safe functions are tagged `SYRP_RT_SAFE` in the header, from api-index data. A C plugin can implement `syrp_processor_vtable`. |
| **C++** | Generated header-only C++17/20 RAII layer over the C ABI. Diplomat 0.17 (MIT/Apache) is evaluated as the generator, since it emits C, C++ and JS from one bridge. `cxx` 1.0 is used only where Rust must call C++ (the AAX shim) or for zero-copy shared types. | Classes such as `syrp::ui::Slider` and `syrp::audio::ProcessorGraph`, `std::span` buffer views, exception-free `expected<T, syrp::Error>` | autocxx is not needed. Lifetime rules are documented per type and enforced by handle generations. |
| **A JUCE-shaped C++ facade?** | — | **Recommendation: do not ship one by default.** | Copying JUCE's header API (class names, method names and signatures, hierarchy) is the riskiest legal item (§10), and it invites semantic mismatch bugs. Instead, ship a **migration kit**: the class-by-class migration map, `cargo syrp migrate` codemods for C++ (clang-tooling based) and Rust, and an Agent Skill that ports JUCE projects to the Syrp C++ or Rust APIs. A thin, opt-in `juce-compat` header could be considered later, only after counsel review (§11 decision). |
| **Python** | PyO3 0.30 plus maturin 1.15 (MIT/Apache) | Offline rendering, DSP, file formats, MIDI and UMP, headless plugin hosting, ValueTree, the parity and inspect tools, buffers as numpy arrays (zero-copy) | Python is never called on the audio thread. Callbacks run on the message thread or offline. The GIL is released during renders. abi3 wheels are built for CPython 3.10 and later. |
| **Swift / Kotlin** | UniFFI 0.32 (MPL-2.0 tool) | App-level APIs on iOS and Android: device management, file formats, UI hosting for native app shells, purchases | RT callbacks stay in Rust. UniFFI's async support covers long operations. |
| **JS / wasm** | wasm-bindgen 0.2.129 plus TypeScript `.d.ts` | Web apps: audio engine, DSP and formats in wasm; UI on canvas | Separate from the Phase 5 WebView JS interop package, which is pure TS (npm). |
| **C#** | csbindgen 1.9 (MIT); interoptopus as the alternative | .NET and Unity scripting over the C ABI | Pairs with the Unity plugin wrapper. Ships as NuGet packages. |

### 8.3 Keeping bindings generated, never hand-written
- **One annotated source.** `syrp-capi` items carry structured doc comments and attributes recording ownership, threading, RT-safety and error codes. Every generator reads them. `api-index.json` records the per-language names.
- **Drift gate.** CI regenerates every binding and fails on any diff. A semver-checks job runs on `syrp-capi`, and an ABI check (symbol and layout tests) catches breaking C ABI changes.
- **Cross-language parity tests.** The parity scenarios carry a `bindings` subset. They run in Rust, C, C++, Python, Swift, Kotlin, JS and C#, and outputs are compared with the Rust goldens.
- **Docs per language** are generated from the same doc comments, with per-language snippets compiled in CI. Agent Skills are published per language (`syrp-cpp`, `syrp-python`).
- **Licensing.** Bindings carry the same terms as the Rust crates (§10.6).

## 9. Testing and parity verification

### 9.1 Test layers
| Layer | Tooling | When it runs |
|---|---|---|
| Unit, property and doctests | nextest, proptest, `cargo test --doc` | Every PR (affected crates) |
| Compile-fail misuse tests | trybuild | Every PR |
| Contract checks | cargo-public-api snapshots, cargo-semver-checks | Every PR |
| Differential vs JUCE | `juce-oracle` plus `cargo syrp parity` | Every PR (affected scenarios), nightly (all) |
| Golden audio and images | testkit comparators | Every PR (affected), nightly (all scales and OSes) |
| Plugin validators | pluginval, clap-validator, Steinberg validator, auval, lv2lint, aaxval | Merge queue and nightly |
| DAW smoke and device loopback | Self-hosted device lab | Nightly and pre-release |
| Fuzzing | cargo-fuzz (libFuzzer), arbitrary | Continuous, with nightly campaigns |
| Miri, loom, TSan and ASan | nightly toolchain | Nightly; on demand for the high-risk label |
| RT safety | Allocation guard, RTSan, timing budgets | Every PR for audio crates, plus nightly |
| Mutation testing | cargo-mutants on changed code | Every PR for core and DSP crates (diff mode) |
| Agent usability evals | Reference agent plus the skill (A20) | Nightly and release gate |

### 9.2 Differential testing against real JUCE
- **`juce-oracle`** is a **separate repository licensed AGPL-3.0**. It builds JUCE at the ledger-pinned tag (9.0.4) from the official repo with CMake (headless builds were improved in JUCE 9). It exposes one CLI: `juce-oracle run <scenario.json> --out <dir>`. That command emits JSON, WAV and PNG.
- Its C++ is written by a separate **"oracle" agent pool** that may read JUCE headers and docs in order to compile against them. These agents have no write access to the Syrp repository, and Syrp implementers never see oracle code (§10.4).
- **Scenarios** are language-neutral JSON, held in `parity/scenarios/`. Examples:
  - "construct a NormalisableRange(20, 20000, skew 0.25), map these 101 points";
  - "render a ProcessorChain of Gain → LadderFilter with these parameters for 48k samples";
  - "paint a Slider with LookAndFeel_V4 at scale 1.5 with value 0.3".
- The same scenario runs through `cargo syrp parity run`, and the comparator applies the scenario's **tolerance profile**.
- **Reference outputs** are produced by the oracle in CI and stored in a separate `syrp-goldens` data repository. Each output records the JUCE tag, platform and renderer used.
- **Observation-only rule.** Scenarios describe inputs and observable outputs, never internals. This is the black-box half of the clean room.

### 9.3 Numeric tolerances for DSP and audio
Tolerances are per profile, stored in `parity/tolerances.toml`, and every scenario names one.

| Profile | Applies to | Criterion |
|---|---|---|
| `exact` | Sample-format conversion, integer PCM codecs (WAV/AIFF/FLAC decode), MIDI and UMP bytes, MidiFile, OSC, ValueTree binary, JSON and XML text | Bit-exact |
| `f32-arith` | FloatVectorOperations, gain and mixing, AudioBuffer ops | max \|Δ\| ≤ 4 ULP, or ≤ 1e-7 absolute near zero (SIMD reassociation) |
| `fft` | FFT and windowing | Relative RMS error ≤ 1e-6 (f32) or ≤ 1e-12 (f64), plus identical scaling and bin ordering |
| `filter` | IIR, FIR, SVF, TPT, LinkwitzRiley, Ladder, Ballistics | Magnitude response within ±0.01 dB and phase within ±0.1° across 20 Hz–20 kHz at 3 sample rates. Time-domain max \|Δ\| ≤ 1e-5 after 1 s of stepped-parameter input. Identical latency. |
| `nonlinear` | WaveShaper, Compressor, Limiter, NoiseGate, oscillators (incl. band-limited variants) | Static curve ≤ 1e-5. Envelope attack and release time within ±1%. Spectrum within ±0.1 dB below −60 dBFS aliasing. |
| `algorithmic` | Reverb, Chorus, Phaser (implementation-defined) | Behavioural equivalence: RT60 within ±5%, spectral envelope within ±0.5 dB per third-octave band, stereo correlation within ±0.05, documented parameter mappings identical. Bit parity would require copying JUCE's algorithm, so it is **not** a goal. |
| `resample` | ResamplingAudioSource, interpolators, Oversampling | Passband ripple ±0.01 dB, stopband ≥ the documented attenuation, SNR ≥ 100 dB versus the reference on sine sweeps, identical reported latency |
| `lossy-decode` | MP3, Vorbis, Opus decoding | max \|Δ\| ≤ 1e-4 against the reference decoder output, plus codec conformance vectors |
| `timing` | Timers, transport, PlayHead, tail and latency reporting | Exact sample positions in offline mode; ±1 ms jitter budget on the live device lab |

### 9.4 Golden images for graphics and UI
- **Rendering.** References are rendered by the oracle with JUCE's software renderer, which is deterministic, and also captured per platform for the native renderers. Syrp renders with `render-cpu` in deterministic mode and, nightly, with `render-gpu`.
- **Comparator.** The comparator in `syrp-testkit` checks four things:

| Check | Threshold |
|---|---|
| Fill regions, per-channel difference | ≤ 2/255 |
| AA edges and text | SSIM ≥ 0.995 inside an edge mask |
| Differing pixels overall | ≤ 0.3% |
| Geometry (from path dumps rather than pixels) | Hausdorff distance ≤ 0.25 device px |

- **Text** has an additional objective check: shaped glyph IDs and advances must be identical to JUCE's HarfBuzz output for the pinned font corpus.
- **Scales and themes.** Goldens are captured at 1x, 1.25x, 1.5x and 2x, and in dark and light LookAndFeel variants.
- **Updating goldens** requires a `golden-update` WP. The PR manager attaches the visual diff as an image artifact, and the adversarial reviewer must approve it. Agents cannot "accept" their own diffs.

### 9.5 Plugin validators
| Validator | Formats | Platforms | Gate |
|---|---|---|---|
| pluginval 2.0.0 (GPL-3.0, run as an external binary) | VST3, AU (and VST2) | Windows, macOS, Linux | Strictness 10 for all example plugins. Fuzz options are enabled nightly. |
| clap-validator 0.4.1 (MIT) | CLAP | Windows, macOS, Linux | All tests pass |
| Steinberg VST3 validator (SDK, MIT) | VST3 | Windows, macOS, Linux | All tests pass |
| `auval -strict` (macOS built-in) | AUv2, AUv3 | macOS | Pass |
| lv2lint (licence UNVERIFIED) | LV2 | Linux | No errors; warnings tracked in the ledger |
| AAX validator (DigiShell `aaxval`, in the AAX SDK) | AAX | Windows, macOS (Pro Tools Developer) | Describe validation plus test suites pass. A user-supplied SDK and an Avid developer account are required. |
| Wrapper fuzzing (Syrp's own host) | All | All | Random call sequences, threads and state blobs. No crash, no RT violation. |

**DAW smoke matrix** (device lab, nightly or pre-release):
- **Reaper:** scriptable, with command-line rendering.
- **Bitwig:** CLAP and VST3.
- **Ableton Live:** VST3 and AU.
- **Logic:** AU.
- **Cubase:** VST3.
- **Pro Tools Developer:** AAX.
- **Ardour and Carla:** LV2 and CLAP.

Reaper, Ardour and Carla are automated through their scripting interfaces. The others run via scripted UI automation and are best effort. Failures open issues automatically.

### 9.6 CI matrix
| Tier | Runners | Jobs |
|---|---|---|
| PR (fast, about 20–40 min) | ubuntu-24.04 x64, windows-2025 x64, macos-15 arm64 (GitHub-hosted; UNVERIFIED image names) | fmt, clippy (deny), nextest on affected crates, doctests, contract snapshots, semver, deny, layer lint, size lint, affected parity scenarios and goldens, clean-room scan |
| Merge queue (full, 60–120 min) | Above plus linux arm64 and windows arm64. Cross-compiles: iOS, Android (aarch64, x86_64), wasm32 (unknown and emscripten). | Full test suite, all scenarios, validators on the platforms they support, docs build, binding generation (Phase 8+) |
| Nightly | Above plus a self-hosted device lab: Mac with audio interface and MIDI 2.0 device, Windows with ASIO interface, Linux with JACK/PipeWire, iPhone, iPad, Android phone and tablet | Miri, loom, TSan/ASan, RTSan, fuzz (1 h per target), mutation, GPU goldens, DAW smoke, device loopback latency, 8-hour soak, agent evals |
| Release | All above | 72-hour soak before 1.0, full validator sweep, signing and notarisation dry run, SBOM (cargo-about, cargo-auditable), cargo-vet |

The toolchain is stable (pinned) plus nightly for sanitisers and Miri. MSRV is checked as "pinned stable minus 2".

### 9.7 Fuzzing
`fuzz/` targets:
- WAV, AIFF, FLAC, Ogg, MP3 and Opus readers;
- MidiFile, MIDI and UMP streams, and MIDI-CI messages, including the encodings;
- the XML, JSON and `var` parsers, and ValueTree binary plus `juce-compat`;
- OSC packets and the Expression parser;
- URL parsing, the SVG to Drawable path and image codecs (shallow, since those are delegated);
- plugin state blobs and IPC messages;
- CLAP, VST3 and AU wrapper call sequences, driven through the Syrp host.

Corpora are seeded from the oracle's outputs and from public conformance vectors. Crash minimisation and regression tests are added automatically by the PR manager.

### 9.8 Miri, loom and sanitisers
- **Miri** runs on every crate without FFI: core, var, state, midi, scalar DSP, and the rt primitives in Miri-compatible mode.
- **loom** models cover the SPSC queues, deferred drop, `RtHandoff`, ParamTree's atomic sync and the graph-plan swap.
- **TSan and ASan** run on FFI crates: peers, wrappers, hosts and the C ABI.
- Every `unsafe` block must carry a `// SAFETY:` comment, and `xtask` counts and lists them in the PR report.

### 9.9 Real-time safety checks
1. **Static.**
   - The token types make allocating APIs unreachable from `RtCtx`.
   - `clippy.toml` `disallowed-methods` blocks Vec growth, `Box::new`, `Mutex::lock`, `println!` and file I/O inside modules tagged `#![rt]`.
   - A custom dylint (`rt_reachability`) walks the call graph from `Processor::process` implementations.
2. **Dynamic.**
   - The allocation guard is on in all audio tests.
   - RTSan runs nightly against every example plugin under the fake host.
3. **Budget.**
   - `process()` must stay under a CPU budget at 64-sample blocks: p99.9 ≤ 30% of the block period on the reference machine.
   - Device-lab xrun counters must be zero over a 1-hour run.

### 9.10 The parity ledger
`parity/ledger.json` is machine-readable and validated against `ledger.schema.json`. It has one row per JUCE public class and one per tooling feature, with nested feature rows. **It is the single source of truth for phase exits, docs (the capability manifest A19) and the migration map.**

```json
{
  "id": "juce_gui_basics/Slider",
  "juce": { "module": "juce_gui_basics", "class": "Slider", "baseline": "9.0.4",
            "doc": "https://docs.juce.com/master/classSlider.html" },
  "syrp": { "crate": "syrp-widgets-slider", "item": "syrp_widgets_slider::Slider" },
  "status": "partial",
  "features": [
    { "id": "rotary-drag-modes", "status": "done", "scenarios": ["ui/slider/rotary_drag"] },
    { "id": "velocity-mode", "status": "in_progress", "wp": "WP-3.2-08" }
  ],
  "platforms": { "windows": "done", "macos": "done", "linux": "done",
                 "ios": "planned", "android": "planned", "web": "planned" },
  "tolerance_profile": "ui-default",
  "evidence": { "differential": "pass", "golden": "pass", "run": "2026-10-20T03:12Z" },
  "migration": "docs/migration/Slider.md",
  "gap": null
}
```

- **Statuses:** `planned`, `in_progress`, `partial`, `done`, `mapped` (covered by std or a Rust idiom, such as `Array` mapping to `Vec`), `gap` (deliberate, with a reason), and `deviation` (semantics intentionally differ, with a reason).
- **Evidence comes only from CI.** A row cannot become `done` unless CI evidence for all of its scenarios is green on all of its declared platforms. Status changes go through the PR manager, and agents cannot hand-edit evidence.
- **Upstream tracking.** A nightly job diffs JUCE's public docs and `BREAKING_CHANGES.md` for new releases and opens "upstream delta" rows automatically.

## 10. Licensing and legal

*This section is a plan, not legal advice. Every recommendation here needs review by counsel qualified in the US, the EU and the UK before launch.*

### 10.1 JUCE's terms today (verified 2026-10-26)
- JUCE modules are dual-licensed: **AGPLv3**, or the commercial **JUCE 9 EULA**. JUCE's examples are ISC (JUCE `LICENSE.md` at 9.0.4; https://juce.com/legal/juce-9-licence/).
- The commercial tiers are the same on the JUCE 8 and JUCE 9 EULA pages (https://juce.com/legal/juce-8-licence/, https://juce.com/get-juce/):

| JUCE tier | Revenue or funding limit (trailing 12 months, entity plus all Affiliates, no offsets) | Price |
|---|---|---|
| Starter | up to **USD 20,000** | Free |
| Indie | up to **USD 300,000** | USD 40/user/month or USD 800 perpetual |
| Pro | No limit | USD 175/user/month (12-month minimum) or USD 3,500 perpetual |
| Educational | — | Free (accredited institutions; non-commercial) |
| AGPLv3 | — | Free for anyone who complies with the AGPL |

- Forum posts from April 2024 put Indie at USD 200k when JUCE 8 launched, down from USD 500k under JUCE 7 (https://forum.juce.com/t/revenue-limits-for-juce-tiers/61058). The current pages say USD 300k, so the limit was evidently raised afterwards. The date of that change is UNVERIFIED.
- For individuals, the EULA counts revenue generated by their use of the Framework. For companies, it counts all revenue and funding of the entity and its Affiliates.
- **EULA clauses that matter for us**, paraphrased by fetch; counsel should confirm the exact wording:
  - 1.12: no "Products that create other Products" without a separate licence.
  - 2.2 and 2.4: no modification or adaptation, and no reverse engineering or derivative works, except where the law permits.
  - 2.3: no actions that subject the Framework to open-source terms.
- **Consequence.** Nobody on the project should accept the commercial JUCE EULA for project work. The differential harness uses JUCE **under AGPLv3** (§10.9).

### 10.2 Goal
"More permissive than JUCE, but only large organisations pay." Concretely:
- **Free for everyone, including closed-source commercial products,** when the developing organisation is under about **USD 100k** of annual revenue plus funding. JUCE's free closed-source tier stops at USD 20k, so this is five times higher.
- **Never more restrictive than JUCE's AGPL path**: anyone can use Syrp free under a strong copyleft licence.
- **End users of products built with Syrp never need a licence.**
- **No splash screen, no telemetry, no phone-home.**
- **Eventual open source**: each version becomes permissive after a fixed delay. JUCE has no equivalent.

### 10.3 Licence models compared
| Model | OSI open source? | How the revenue test is defined | Enforcement practicality | Fit with crates.io and the Rust ecosystem | Precedent | Fit for the goal |
|---|---|---|---|---|---|---|
| **BUSL-1.1 with a revenue-gated Additional Use Grant (AUG)** (https://mariadb.com/bsl11/) | No. SPDX lists `BUSL-1.1` and says it "is not an Open Source license" (https://spdx.org/licenses/BUSL-1.1.html). Each version converts to an open-source Change License on its Change Date (≤ 4 years). | Free text in the AUG, so the licensor defines it: revenue and/or funding, affiliates, period. | Honor system plus legal risk for large organisations. Commercial customers sign contracts with audit rights. | crates.io accepts any SPDX expression; `LicenseRef-` works too (e.g. `truce` publishes `LicenseRef-TruceLicense-1.0`). cargo-deny denies anything not on a project's allow-list (https://embarkstudios.github.io/cargo-deny/checks/licenses/cfg.html), so adopters must allow-list it. Distros will not package it before conversion. | **Akka**: BSL 1.1 since 2.7 (2022), free for organisations under USD 25M revenue, Apache-2.0 after 3 years (https://akka.io/blog/lightbend-changes-its-software-licensing-model-for-akka-technology). **Directus**: BSL 1.1, free under USD 5M "Total Finances" (the largest of gross revenue, budget or funding) (https://scancode-licensedb.aboutcode.org/directus-bsl-1.1.html). Directus moved to a new licence in 2026 (https://directus.com/license). | **Strong.** Tunable to USD 100k; a recognised template; built-in path to open source. |
| **PolyForm Small Business 1.0.0** (https://polyformproject.org/licenses/small-business/1.0.0/) | No | Fixed: **< 100 workers *and* < USD 1M (2019, CPI-adjusted) revenue**, affiliates included | Honor system; 32-day cure period | Same as BUSL, with a LicenseRef or SPDX id (`PolyForm-Small-Business-1.0.0`) | Drone Autoscaler, with a USD 1M revenue test (https://autoscale.drone.io/faq/licensing/is-this-project-open-source/) | **Weak.** The threshold is 10× the target and cannot be changed without forking the licence. There is no explicit end-user carve-out for embedded copies in distributed products, and no conversion to open source. |
| **Big Time Public License 2.0** (https://writing.kemitchell.com/2022/01/26/Big-Time-2.0.0.html) | No | Fixed: < 20 workers, < USD 1M revenue and < USD 1M of investment over 5 years. Large businesses get 128 days, then must obtain a FRAND licence. If the licensor fails to offer FRAND terms, use becomes free. | Honor system | As above | Few known adopters (UNVERIFIED) | **Medium.** The FRAND duty is fair, but thresholds are fixed and unfamiliar to legal teams. |
| **FSL-1.1 / FCL** (Fair Source) (https://fsl.software/, https://fair.io/licenses/) | No (converts to Apache-2.0 or MIT after 2 years) | **No revenue test.** The only restriction is a non-compete; FCL adds licence keys. | Easy | Same as BUSL | Sentry (FSL), Keygen (FCL) | **Poor.** It does not make large organisations pay. It only stops competitors. |
| **Custom "community licence" plus a commercial dual licence** | No | Fully custom | Same as BUSL | `LicenseRef-` only, so legal teams have to read a novel text | Many proprietary SDKs | **Good but costly.** Bespoke drafting, no recognisability, slower adoption. |
| **AGPLv3 plus commercial** (JUCE's model) | Yes, through the AGPL option | None. You pay to escape copyleft. | Through copyleft | OSI-approved option; many companies ban AGPL | JUCE, Qt | **Fails the goal.** Small closed-source developers would have to pay. |
| **MIT/Apache-2.0** | Yes | None | — | Ideal | Most of the Rust ecosystem | **Fails the goal.** No one pays. |

### 10.4 Recommendation
License the framework as **`AGPL-3.0-only OR BUSL-1.1`**:
- The BUSL instance has a **revenue-gated Additional Use Grant**, a **Change Date of 3 years after each version's release**, and a **Change License of `Apache-2.0 OR MIT`**. MIT satisfies BUSL's requirement that the Change License be GPLv2-or-later compatible.
- Organisations above the threshold that do not want the AGPL take a **commercial licence**.

**Rules for the AGPL option.** If Syrp is used under AGPL-3.0-only, two rules follow:
- **The licence passes on.** The whole program, including all of its own source code, must be released under the AGPL-3.0 when it is distributed.
- **Network use counts as distribution.** If users interact with the program over a network, they must be offered its source too. This network rule is what separates the AGPL from the GPL.

Three further measures:
- **Open interfaces.** The L0 and L1 crates are licensed **`MIT OR Apache-2.0`**: `syrp-error`, `-rt`, `-geom`, all `*-api` contracts, `syrp-midi`, plus the TypeScript WebView interop package. Third-party widget, DSP and plugin libraries can then target Syrp's interfaces without inheriting the revenue gate. Only implementations are gated.
- **A paid tier above the threshold.** Pricing is a user decision. A starting proposal is to undercut JUCE Pro (USD 175/user/month): per-seat pricing with an organisation cap, plus an enterprise tier with indemnity, an SLA and LTS.
- **Why this combination.** It is a recognised template that legal teams can approve quickly. It is never more restrictive than JUCE: both offer an AGPL option, Syrp's free closed-source tier is 5× larger, and Syrp versions eventually become permissive. Large organisations still pay.

**Draft key terms of the Additional Use Grant** (for counsel to finalise):
> **Additional Use Grant.** You may make production use of the Licensed Work, including incorporating it into and distributing Products, at no charge, if you are a **Qualifying User**.
>
> **1. Qualifying User** means any of:
> - **(a) Small entity.** A legal entity whose **Gross Receipts**, aggregated with all of its **Affiliates**, were below **USD 100,000** in both the most recent completed fiscal year and the trailing 12 months. **Gross Receipts** means all revenue from any source plus all external funding received (equity, convertible or other debt financing, grants, donations, sponsorship), without offsets. **Affiliate** means any entity controlling, controlled by, or under common control with you.
> - **(b) Individual.** An individual acting on their own behalf whose receipts *from Products or services that use the Licensed Work* were below that amount in the same periods.
> - **(c) Education.** An accredited educational institution, for teaching or non-commercial research.
> - **(d) Non-profit.** A registered non-profit whose total annual budget is below that amount.
>
> **2. Work for clients.** Work done for, or on behalf of, a client is measured against the client's Gross Receipts.
>
> **3. Grace period.** If you stop being a Qualifying User, this grant continues for **90 days**. Copies of Products already distributed to end users stay licensed.
>
> **4. End users.** Use of a Product by its end users is always permitted, regardless of the end user's size, and needs no licence from the Licensor.
>
> **5. Non-production use.** Evaluation, testing, education and other non-production use are always permitted, as in BUSL's base grant.
>
> **6. No reporting.** No registration, telemetry or reporting is required. You may self-attest with `cargo syrp license attest`, which writes a local file only.

### 10.5 Contribution model when agents write most of the code
- **Ownership of agent output.** Selling commercial licences requires the steward to hold, or be licensed, every protectable right in the code. For code generated by the steward's own agents, the steward is the "customer" under its AI providers' terms. Anthropic's Commercial Terms, Section B, say: "Anthropic hereby assigns to Customer its right, title and interest (if any) in and to Outputs" (https://www.anthropic.com/legal/commercial-terms). Other providers' terms must be checked for each provider used.
- **Run all agents under the steward's accounts.** No CLA is needed for the steward's own agents. What is needed is **provenance records**: commit trailers `WP:`, `Spec:`, `Agent-Model:`, `Reviewed-By:` and `Human-Edits:`, plus retained prompts, specs and review logs.
- **External contributors**, whether humans or their agents, must sign a **CLA**. This is an Apache-ICLA-style licence grant with an explicit right to sublicense and relicense, including commercially, plus a patent licence. It also includes three representations:
  - that the contributor has the rights to any AI-assisted output;
  - a disclosure of which AI tools were used;
  - that no code was copied from incompatible sources, and that the contributor has not studied JUCE source for the area contributed (clean room).
- **Mechanics.** A CLA bot plus DCO sign-off enforce this automatically, keeping the pipeline hands-off. A licence-with-relicensing grant is enough; copyright assignment is not required.
- **What a CLA cannot do.** It cannot create copyright where none exists. It only consolidates whatever is protectable.

### 10.6 AI-authorship copyright risk, and how to keep the business defensible
**Current law**

| Jurisdiction | Position |
|---|---|
| **US** | Human authorship is required. *Thaler v. Perlmutter*, 130 F.4th 1039 (D.C. Cir. 2025) affirmed this, and the **Supreme Court denied certiorari on 2026-03-02** (https://www.mayerbrown.com/en/insights/publications/2026/03/supreme-court-denies-review-in-ai-authorship-case). The Copyright Office's **Part 2 report on copyrightability (2025-01-29)** concludes: prompts alone are insufficient; human expressive inputs that remain perceptible in the output, and creative human selection, arrangement or modification, can be protected; assessment is case by case (https://www.copyright.gov/ai/; summary: https://sidley.com/en/insights/newsupdates/2025/02/us-copyright-office-issues-report-on-artificial-intelligence-and-copyrightability). *Allen v. Perlmutter* is pending. |
| **EU** | Protection requires the author's own intellectual creation. A European Parliament study (July 2025) concludes that fully AI-generated output generally fails this test, and that AI-assisted work may qualify where a human substantially shapes it (https://www.jonesday.com/en/insights/2025/08/european-parliaments-new-study-on-generative-ai-and-copyright-calls-for-overhaul-of-optout-regime). |
| **UK** | CDPA s9(3) still protects "computer-generated works" today. The government's 2026-03-18 report proposes **removing** that protection, with no timetable (https://www.stephensonharwood.com/post/102mpke/uk-government-publishes-report-and-impact-assessment-on-copyright-and-artificial-intelligence-the-debate-continues/). |

**Implication.** In a hands-off, mostly agent-written codebase, large portions may not be protected by copyright in the US or EU. The licence conditions, both the BUSL revenue gate and the AGPL's copyleft, may then be unenforceable against copying those portions. **The business model must not rely on copyright alone.**

**Measures, in order of value**
1. **A human-authored spine.** Budget a small amount of human authorship, about 4–8 hours a week of a designer or architect, on the most valuable expressive layers:
   - the **contract crates**: public API design, trait and type definitions, doc comments;
   - the **architecture and design docs**;
   - tutorials and the cookbook;
   - the **LookAndFeel visual designs**.

   Humans write or substantially rewrite these, so they are protectable as human selection, arrangement and expression, and they show perceptibly in what agents build. Each human edit is logged (`Human-Edits:` trailer).
2. **Human-written specs and examples.** These are expressive inputs that remain perceptible in outputs.
3. **Compilation and arrangement.** The curated structure of the framework (crate decomposition, module organisation, catalogues) is a human-directed selection and arrangement. Its protection is thin but real.
4. **Trademarks.** Register the name and logo (classes 9 and 42; US, EU, UK). Publish a trademark policy: forks may not use the name, and "Certified/Built with Syrp" badges are licensed. Trademarks do not depend on authorship.
5. **Contracts.** The commercial licence is a contract and binds the licensee whatever the copyright status. Enterprise value is delivered through services rather than code:
   - indemnity and warranties;
   - an SLA, LTS branches and security backports;
   - compliance artefacts (SBOMs, provenance);
   - help with AAX and PACE onboarding;
   - priority fixes.
6. **Paid hosted tooling.** A "parity cloud" covering the device lab, DAW matrix and validator farm as a service; hosted MCP with organisation context; managed migration of JUCE codebases by agents.
7. **Trade secrets.** Keep the orchestrator, device-lab automation, extended golden corpora and internal prompts unpublished.
8. **Fast release cadence.** Commercial customers get the update stream with SLAs, and the community gets the same code. Speed, not secrecy, is the moat.

### 10.7 How the licence carries into interop and plugins
- **Bindings use the same terms.** The C ABI, the generated C++ layer, the Python wheels, the UniFFI packages, the wasm/npm package and NuGet packages are all part of the Licensed Work and carry `AGPL-3.0-only OR BUSL-1.1`.
  - The interface-only crates stay MIT/Apache.
  - PyPI and npm accept SPDX expressions. Whether NuGet accepts non-OSI license expressions is UNVERIFIED; if not, ship a licence file.
- **GPL/AGPL programs** use the AGPL option. GPLv3 and AGPLv3 combine under GPLv3 §13. GPLv2-only programs are incompatible until a version reaches its Change Date (then Apache-2.0 OR MIT).
- **Proprietary programs** use the BUSL grant under the threshold, and a commercial licence above it. Dynamic linking does not change this: BUSL governs use, not linking mode.
- **Plugin SDK constraints on plugins built with Syrp:**

| Format | Constraint |
|---|---|
| **VST3** | MIT since 3.8.0. No Steinberg agreement needed; follow the VST logo and usage guidelines (UNVERIFIED terms). |
| **CLAP** | MIT. |
| **AU** | Apple frameworks; the AudioUnitSDK is Apache-2.0. |
| **LV2** | ISC. |
| **ARA** | Apache-2.0. |
| **AAX** | The SDK is "commercial or GPLv3" (Avid licence text in JUCE's bundled copy). Syrp **never redistributes it**: `syrp-wrap-aax` contains only Syrp's shim and builds against a user-supplied SDK path. AGPL-route developers may combine with the GPLv3 AAX option. Closed-source developers need Avid's commercial AAX agreement. Everyone shipping to retail Pro Tools needs PACE signing through an Avid developer account. |
| **ASIO** | GPLv3 or a Steinberg proprietary agreement. Also user-supplied. Closed-source apps need Steinberg's ASIO licence. |
| **VST2** | Not offered. |
| **Unity** | Terms of the native audio plugin SDK header are UNVERIFIED. |

### 10.8 API surface, naming and trademarks
- **Copyright background.**
  - US: *Google v. Oracle* (2021) held that reimplementing Java's declaring code was fair use on those facts (https://copyrightalliance.org/copyright-cases/oracle-america-v-google/).
  - EU: *SAS Institute v World Programming* (CJEU C-406/10) holds that functionality, programming languages and data-file formats are not protected as such (https://en.wikipedia.org/wiki/SAS_Institute_Inc_v_World_Programming_Ltd).
  - Neither case licenses copying JUCE's header *expression*, such as its doc comments, nor a one-to-one replication of its class and method structure.
- **Policy.**
  - Rust-idiomatic APIs: no mirrored hierarchies or method-by-method signatures.
  - Generic domain names are fine (Slider, Graphics, Path, AudioBuffer, MidiMessage).
  - Distinctive JUCE-coined compounds are renamed, with JUCE names kept only as `#[doc(alias)]` and in the migration map, as nominative references. Examples: AudioProcessorValueTreeState → `ParamTree`. *ValueTree* and *LookAndFeel* are working names in this plan, pending counsel's naming review; `StateTree` and `Theme` are the alternates.
- **Trademarks.**
  - "JUCE" is used only nominatively ("migrating from JUCE"). No JUCE logos, and no "JUCE-compatible" product naming. Registration details of the JUCE mark are UNVERIFIED.
  - The VST, Audio Units, AAX, Pro Tools and CLAP marks follow their owners' guidelines.

### 10.9 Clean-room methodology
**Two rooms, enforced by tool permissions.**

| Room | Who | May do | May not do |
|---|---|---|---|
| **Spec room** | Spec agents and oracle agents | Read public JUCE documentation, tutorials and forum posts. Run JUCE binaries. Oracle agents may read JUCE headers in order to compile the harness. | Write to the Syrp repository. Spec agents may not open JUCE `.cpp` files or headers. |
| **Clean room** | Implementers, reviewers and the PR manager | Read the WP specs (written in the spec room's own words), scenario outputs and Syrp's own code. | Access juce.com, docs.juce.com, github.com/juce-framework, forks and mirrors of JUCE, or the `aloe-*` crates. |

**Enforcement and evidence**
- The clean room's web sandbox blocklists the JUCE domains listed above. Dependency bans are enforced by `cargo-deny`.
- **Specs describe behaviour, never implementation.** Constants and tables come from observation, such as oracle-measured LookAndFeel colours and default parameter values, with provenance recorded. They never come from recall.
- **Memorisation risk.** LLMs may have seen JUCE source in training, so a quarantined **contamination scanner** compares each PR diff with JUCE source using token winnowing, as MOSS and JPlag do. It is the only process that touches JUCE source, and it reports only similarity scores and line ranges. Scores above the threshold block the merge and trigger re-implementation from spec by a different agent.
- **Reviewer checklist.** Reviewers look for JUCE-style idioms, comments or magic numbers without provenance.
- **Audit trail.** Logs of rooms, specs, scanner results and provenance are retained for legal defensibility.

### 10.10 What testing against JUCE implies
- `juce-oracle` links JUCE, so it is an AGPL-3.0 work. It lives in its own repository under AGPL-3.0. If it is ever distributed, its source is published. It is never shipped with Syrp.
- Running JUCE under AGPLv3 avoids the commercial EULA's no-reverse-engineering clause. Observing program outputs is not decompilation.
- **Reference outputs** (numbers, WAVs, PNGs) are machine outputs and probably not copyrightable. They are still kept in a separate `syrp-goldens` repository with a notice ("generated by a JUCE-based harness under AGPLv3"), and Syrp's tests fetch them as data.
- Using real JUCE output as a *test oracle* does not make Syrp a derivative work, provided no JUCE code or expression enters Syrp. The clean-room evidence above supports that position.

## 11. Risks and open questions

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| R1 | Scope is underestimated: JUCE's breadth and the long tail of host quirks | High | High | Ledger-driven scope; rolling waves; parity sweeps per phase; deliberate gaps recorded early; device lab from Phase 2 |
| R2 | Text or AA rendering fidelity is not reachable with vello and parley | Medium | High | Phase 0 renderer spike; tolerance profiles; tiny-skia fallback; native D2D or CoreGraphics backends as a contingency |
| R3 | The plugin host-compatibility long tail | High | High | DAW smoke matrix; host-quirk tables as data; beta program with a real plugin vendor; validators at maximum strictness |
| R4 | Clean-room contamination through model memorisation | Medium | Very high | Room separation; contamination scanner; provenance; re-implementation on a hit; counsel review |
| R5 | Thin copyright undermines the revenue-gated licence | High | High | §10.6: human-authored spine, trademarks, contracts and services, trade secrets |
| R6 | Adoption friction from a non-OSI licence (cargo-deny allow-lists, corporate policies, distros) | High | Medium | AGPL option; MIT/Apache interface crates; 3-year conversion; clear FAQ; `cargo syrp license` |
| R7 | Churn in pre-1.0 dependencies (vello 0.x, parley 0.x, clack 0.2, vst3 0.3, accesskit 0.x, wgpu majors), with no route to upstream agent PRs (RustAudio AI policy) | High | Medium | Wrap at boundaries; pin; fork-and-own with attribution; human-authored upstream issues and patches only |
| R8 | Platform churn: macOS/iOS 26+, the Windows MIDI Services rollout, Wayland, Android 16+ | Medium | Medium | Nightly upstream-delta jobs; peer-api isolation |
| R9 | JUCE moves the target: APv2, CLAP, Wayland, multi-processor binaries | Medium | Medium | Ledger tracks upstream deltas; lead on CLAP, sample-accurate automation and UMP |
| R10 | Hardware and DAW testing is the throughput bottleneck | Medium | Medium | Device-lab capacity plan; scriptable DAWs first (Reaper, Ardour, Carla); prioritised queue |
| R11 | Human-gated external processes: Apple notarisation and certificates, Avid and PACE accounts, store accounts | High | Low–Medium | Humans hold accounts. Automation uses API keys. These are explicit escalation items (§12). |
| R12 | Agent-pipeline failure modes: runaway loops, cost blow-ups, rubber-stamp reviews | Medium | Medium | Orchestrator budgets, leases, circuit breakers; audit sampling (§12) |
| R13 | Security: supply chain, untrusted plugins in the host, the WebView bridge | Medium | High | cargo-vet and cargo-audit; out-of-process scanning; sandboxed WebView origins; fuzzing |
| R14 | Performance below JUCE in hot paths | Medium | Medium | Benchmarks against the oracle; SIMD WPs; performance budget gates |

**Open questions** (answers are needed from research or spikes, not from the user):
- Can vello_cpu's deterministic mode be bit-stable across x86 and ARM, or do goldens need per-architecture baselines?
- Which parts of JUCE's ValueTree binary and plugin-state envelopes can be specified from black-box observation alone? Is any interoperability exception needed (EU Software Directive Art. 6)? Counsel should confirm.
- Can CLAP and VST3 hosts on Wayland embed plugin UIs at all in 2027, or is XWayland the permanent answer? (UNVERIFIED)
- Does the Unity native-audio-plugin SDK header licence allow redistribution of a Rust reimplementation? (UNVERIFIED)
- What do the AAX SDK's NDA and agreement terms require for automated CI in a device lab? (UNVERIFIED)
- How mature are Web Audio Modules for a web plugin target? (UNVERIFIED)

**Decisions the user needs to make**
1. **Licence.** Adopt `AGPL-3.0-only OR BUSL-1.1`, with the AUG free under USD 100k of revenue plus funding, conversion to Apache-2.0 OR MIT after 3 years, MIT/Apache interface crates and a commercial tier above the threshold? Alternatives: BUSL-only (no AGPL option), PolyForm Small Business, or a bespoke community licence.
2. **Threshold details.** Confirm: revenue *plus* funding; Affiliates included; trailing 12 months; a 90-day grace period; the end-user carve-out; free for education and small non-profits; commercial pricing.
3. **Name and trademarks.** Pick the final name (Syrp is a placeholder) and approve trademark filings.
4. **Parity baseline.** JUCE 9.0.4, as recommended, or the user's original 8.x (8.0.15)?
5. **Minimum OS targets** (§6), such as macOS 11 rather than JUCE's 10.11.
6. **VST2.** Accept it as a deliberate gap?
7. **JUCE-shaped C++ facade.** No by default (recommended), or pursue one after counsel review?
8. **Budgets.** Agent compute per month, the device lab, DAW licences, signing and developer accounts.
9. **Escalation contacts and channel** for §12 (who is paged, and where).
10. **External contributions.** Open them, with the CLA, from Phase 1, or only after 1.0?
11. **Renaming JUCE-coined terms** (ValueTree → StateTree?, LookAndFeel → Theme?), after counsel's review.

## 12. Multi-agent development workflow

**Design goal: hands-off by default.** Every step from work decomposition to crates.io publishing runs without a human. PRs are rarely merged on a single check. Agents and tools do the checking, and several independent signals are required for every merge. Humans are an **escalation path** for a short list of defined cases (§12.9).

### 12.1 Roles
| Role | Count | Inputs | Outputs | Tools | Permissions |
|---|---|---|---|---|---|
| **Orchestrator** ("conductor"). A deterministic service, not an LLM. | 1 | `work/waves/*.toml`, WP specs, GitHub state, CI results, ledger | Agent spawns, leases, restarts, wave and phase transitions, escalations | Agent runtime API (for example the Claude Agent SDK running agents headless in containers), GitHub API, `cargo syrp` library | Creates and destroys agent sandboxes; edits labels and assignees; cannot merge or push code |
| **Planner / spec agents** (spec room) | 1–2 | Public JUCE docs, tutorials, forum; oracle outputs; ledger | WP specs (behaviour, in their own words), parity scenarios, wave plans, answers to implementer questions | Web (JUCE domains allowed), `juce-oracle` runner, `cargo syrp parity` | Write `work/` and `parity/scenarios/` through PRs only. **No JUCE source.** |
| **Oracle agents** (spec room) | 0–1 | JUCE public headers and docs, scenarios | Harness code in the separate AGPL `juce-oracle` repo, reference outputs | C++ toolchain, CMake | No access to the Syrp repository |
| **Implementer agents** | 10–15 per wave | One leased WP: spec, frozen contracts, scenarios, crate briefs | 1–4 PRs per WP, filled PR template JSON, tests, docs, `changes/*.json` | `cargo syrp` (check, test, render, parity, docs, explain, wp), MCP, Agent Skill, git | Write only to their WP's crate path (per `ownership.toml`) in a dedicated worktree and branch. No shared files, no `contracts/`, no goldens or ledger evidence. Web sandbox blocks JUCE domains. |
| **Reviewer agents** | 2–4 (pool) | PR, WP spec, checklist, CI artefacts | Structured review JSON posted as a PR review (approve or request changes) | `cargo syrp review`, a fresh sandbox checkout, the full test tooling | Read the repository; comment and approve; cannot push to the PR branch or merge |
| **Adversarial verifier** | 1–2 (different model family or configuration from the primary reviewer) | High-risk PRs (§12.8) | A second independent review plus attack artefacts: extra tests, fuzz seeds, misuse cases | Same tools as reviewers, plus Miri, sanitisers, validators, the contamination report | Same as reviewer |
| **PR manager agent** | 1 | All PRs, CI, merge queue, ledger, change requests | Merges (via the merge queue), rebases, labels, ledger updates from CI evidence, shared-file changes, release PRs, release notes | GitHub API, merge queue, `cargo syrp` (`parity status --update-from-ci`, `docs build`, `release`) | **Sole merger.** Sole editor of shared files: workspace `Cargo.toml`, CI config, `deny.toml`, `ownership.toml`, root AGENTS.md, the ledger schema |
| **Auditor agent** | 1 | Merged PRs (sampled) | Retrospective audits; follow-up WPs; reviewer-quality KPIs | Reviewer tools | Read and comment only |
| **Humans** | About 0.5 FTE | Escalation issues | Decisions; account and credential actions | GitHub, chat | Admin. Normally passive. |

### 12.2 Work decomposition and frozen contracts
- **Sizing.** Each WP covers exactly one crate, or one sub-module of a large crate, and is delivered in 1–4 PRs of at most 1,200 changed lines each.
- **Who writes the specs.** The planner writes the specs for the next wave while the current wave runs. A dependency checker verifies that every WP in a wave depends only on merged or frozen crates (§5.3).
- **Contracts come first.** Before a wave starts, the `*-api` crates it touches are frozen as public-API snapshots. A **contract-change WP** is the only way to alter a frozen contract. It requires the adversarial verifier plus an automated impact analysis listing affected crates and WPs, and it escalates if more than 5 in-flight WPs are affected.
- **Machine-readable spec**, validated by `work/wp.schema.json`:

```toml
id          = "WP-3.2-08"
title       = "syrp-widgets-slider: Slider (linear, rotary, two/three-value, velocity mode)"
crate       = "crates/ui/widgets-slider"
depends_on  = ["WP-3.2-01", "WP-3.2-06"]            # ui core, laf
contracts   = ["syrp-ui@3.2-freeze", "syrp-laf@3.2-freeze"]
ledger_rows = ["juce_gui_basics/Slider", "juce_gui_basics/SliderParameterAttachment"]
spec        = "work/specs/WP-3.2-08.md"             # behaviour only, spec-room authored
scenarios   = ["ui/slider/*"]                       # must pass at tolerance profile below
tolerance   = "ui-default"
risk        = ["public-api"]                        # triggers adversarial review
acceptance  = "cargo syrp wp check WP-3.2-08 --json"
platforms   = ["windows", "macos", "linux"]
budget      = { agent_hours = 24, max_review_rounds = 3, max_restarts = 2 }
```

### 12.3 Orchestration with no human in the loop
- **Scheduling.** The conductor loads the current phase's waves and computes the ready set: WPs whose `depends_on` have merged and whose contracts are frozen. It keeps **10–15 implementer slots** busy, prioritising critical-path WPs first, then by risk, then by size. Waves roll (§7.1).
- **Spawning and assignment.** For each ready WP the conductor:
  - creates a sandbox with a git worktree on `wp/<id>`, a clean-room network policy and the toolchain image;
  - takes a **lease**: the GitHub issue is assigned to the bot, with a `lease:<agent-id>` label, a TTL of 4 h and heartbeats;
  - starts the implementer with a prompt template, the WP spec, crate briefs and the Agent Skill.
- **Health.** If heartbeats stop, a budget is exceeded or the agent loops (the same failing check 5 times), the conductor kills the agent. It restarts a **fresh** agent with a structured failure summary, up to `max_restarts`. After that, the WP is re-planned: the planner splits it or clarifies the spec. A third failure escalates (E5).
- **Review dispatch.** When a PR is marked ready and pre-checks pass, the conductor assigns a reviewer, plus the adversarial verifier for risk-labelled PRs. Reviewers are always a different agent instance from the implementer, and adversarial verifiers use a different model or configuration.
- **Phase completion.** The **exit evaluator** is deterministic code. It queries the ledger (status and CI evidence), CI history (main green for 5 consecutive days), validator results and agent-eval KPIs against the phase's exit criteria (§7.2), and writes `reports/phase-N-exit.json`.
  - On pass: tag `phase-N-complete` and start the next phase's waves.
  - On fail: generate parity-sweep WPs from the gaps automatically.
- **Budgets.** There are compute budgets per WP, wave and phase. At 120% of budget the conductor throttles; at 150% it escalates (E5).

### 12.4 Claiming work without collisions
- **One crate per agent.** `ownership.toml` maps paths to WPs or roles. CI rejects PRs that touch paths outside the WP's ownership.
- **Shared files** are changed only through **change requests**: an issue form with a JSON payload, such as "add dependency X to the workspace" or "add CI job Y". The PR manager applies them in batches. Workspace members are globbed (`members = ["crates/*/*", "apps/*"]`), so adding a crate needs no root edit.
- **Generated artefacts** are regenerated by the merge-queue job, never committed by implementers. This removes the main source of conflicts: API index, docs, ledger evidence and binding outputs.
- **Utilities.** Generic helpers may only live in foundation crates. An implementer who needs a new one files a change request instead of writing a local copy (§12.11).

### 12.5 Review protocol
The reviewer **runs everything itself** in a fresh sandbox and never trusts the implementer's logs. `cargo syrp review <PR>` produces the checklist JSON. Each item is pass or fail with evidence attached:

1. **Spec conformance.** Every behaviour statement in the WP spec maps to at least one test (traceability IDs in test names).
2. **JUCE-semantics conformance.** All WP scenarios pass at the declared tolerance. The reviewer also runs a random 10% sample of the crate's other scenarios. Ledger rows are proposed correctly.
3. **Tests.** Unit tests, property tests where applicable, compile-fail tests for misuse, and a mutation score at or above threshold on changed functions.
4. **Real-time safety.** No reachable allocation or locking from `RtCtx`; allocation-guard tests present; `RtSafe` claims justified.
5. **API conventions.**
   - Naming per the style guide.
   - Thread-affinity tokens.
   - Errors carry codes.
   - No new public items outside the frozen contract.
   - `#[doc(alias)]` set for JUCE names.
   - Files under the size limit; no macro additions without justification.
6. **Docs.** Every public item documented with a compiling example; error catalogue entries; crate AGENTS.md and brief updated; `changes/*.json` present.
7. **Clean room and licensing.** Contamination score below threshold; no JUCE-style comments or constants without provenance; dependency licences within policy; provenance trailers present.
8. **Unsafe and FFI** (if present). `// SAFETY:` on every block; Miri or sanitiser runs clean.
9. **Performance.** Benchmarks within budget where the WP declares them.

**Feedback rounds.**

| Stage | What happens |
|---|---|
| Rounds 1–3 | The primary reviewer reviews; the implementer fixes. |
| After round 3 without approval | A **second independent reviewer** reviews from scratch, for up to 2 more rounds. |
| Reviewers still disagree | The **adversarial verifier arbitrates**, using the spec and the evidence. |
| Arbitration fails, or the dispute is about the spec | Escalate to the spec room. If it is still unresolved, escalate to a human (E1). |

### 12.6 The PR manager's job
**Conventions**
- Branches: `wp/WP-3.2-08-slider`.
- PR titles: `[WP-3.2-08] widgets-slider: velocity mode`.
- Every PR has a YAML/JSON front matter recording: WP, risk categories, contracts touched, ledger rows, scenarios, dependencies added, unsafe count.

**Labels:** `phase:*`, `wave:*`, `wp:*`, `risk:{public-api,unsafe,ffi,plugin-wrapper,licensing,ci-infra,contract-change}`, `status:*`, `golden-update`, `flaky`, `needs-human`.

**Responsibilities**
- **Dependency ordering.** Stacked PRs are queued in the WP dependency order, and dependents are rebased automatically after a parent merges.
- **Merge queue.** GitHub merge queue with batches of up to 5, a full matrix per batch, and automatic bisection on failure.
- **Rebases and conflicts.** The PR manager rebases mechanically. A semantic conflict, where tests fail after a rebase, goes back to the owning implementer as a new review round.
- **CI triage.**
  - Each failure is classified as a regression, flake, infrastructure or environment problem.
  - Flakes (pass on retry, or fail rate above 1% in history) get the `flaky` label, are quarantined from merge gating, and spawn an automatic "fix flake" WP. They are never silently ignored.
  - If main turns red, the PR manager reverts the culprit within 30 minutes.
- **Ledger upkeep.** Ledger statuses and evidence are updated only from CI artefacts (`parity status --update-from-ci`). The PR manager regenerates docs and API indices.
- **Release notes.** Compiled from `changes/*.json` with git-cliff.
- **Shared-file change requests.** Applied in batched PRs, which go through the adversarial verifier because they carry the `ci-infra` risk.

### 12.7 Repo scaffolding this setup needs
- **Root `AGENTS.md`**:
  - mission;
  - clean-room rules;
  - the repository map;
  - the closed-loop commands;
  - the definition of done;
  - conventions: naming, errors, threads, RT, file size;
  - what you may edit, and how to file a change request;
  - commit and PR format, with provenance trailers;
  - escalation etiquette (`wp submit --blocked`).
- **Per-crate `AGENTS.md`**: purpose, JUCE ledger rows, invariants, thread affinity, stability level, how to test, known gaps. `CLAUDE.md` holds `@AGENTS.md` plus the skill path.
- **Issue forms**: WP, bug, parity gap, change request, escalation, golden update. **PR template** with machine-readable front matter.
- **Task board**: GitHub Projects with the fields Phase, Wave, WP, Crate, Status, Lease, Risk, Review round, Budget used. It is synced *from* the `work/` files, which are the source of truth.
- **`CODEOWNERS`** maps `contracts/`, shared files and `.github/` to the bot teams `@syrp-bots/pr-manager` and `@syrp-bots/adversarial`. Together with `ownership.toml`, it drives the required reviews.
- **Branch protection**: main accepts merges only from the merge queue, with required checks G1–G11 (§12.8). The PR manager bot is the only identity allowed to enqueue.
- **Contract and interface crates** (`*-api`) with frozen snapshots in `contracts/`.
- **`xtask`** lints: layers, file size, ownership, AGENTS.md headings, provenance trailers, docs freshness.

### 12.8 Merge policy: every gate automated, never a single signal
A PR merges only when **all** applicable gates pass on the merge-queue candidate. The candidate is rebased onto the latest main.

| Gate | Check |
|---|---|
| G1 | The full CI matrix is green (§9.6), including cross-compiles. |
| G2 | The primary reviewer has approved with every checklist item passing, and has attached its own run logs. |
| G3 | Parity: the WP's scenarios pass, no other ledger row regresses, and the ledger update was derived from CI evidence. |
| G4 | Contract and API: the public-API diff against the frozen snapshot is empty, or there is an approved contract-change WP; cargo-semver-checks passes; the API index has been regenerated. |
| G5 | Supply chain and licences: `cargo-deny` (licences, bans, advisories, sources) and `cargo-vet` pass. A new dependency needs adversarial approval. |
| G6 | Clean room: the contamination score is under threshold and provenance trailers are present. |
| G7 | RT safety: allocation-guard tests and RT lints pass for audio crates. A nightly RTSan regression opens a P1 WP and can trigger an automatic revert. |
| G8 | Docs: missing-docs, doctests, examples, error-catalogue entries, AGENTS.md updates, the docs build and the link check all pass. |
| G9 | Quality: clippy `-D warnings`, the size and layer lints, coverage at or above 80% for new code in core and DSP crates, and a mutation-score threshold on changed functions. |
| G10 | **High-risk categories** (auto-labelled from paths and diff analysis): a **second, independent adversarial review** plus category suites (table below). |
| G11 | The merge-queue batch re-test passes. |

**High-risk category suites for G10**

| Category | Extra suite |
|---|---|
| Public API | API rubric and semver review |
| `unsafe` | Miri, an unsafe budget, and an audit of every `SAFETY` comment |
| FFI | ASan/TSan and ABI tests |
| Plugin-format wrappers | The full validator matrix plus a DAW-smoke subset |
| Licensing-sensitive changes | Licence scanner plus a clean-room auditor pass |
| CI/infra | Dry run on a sandbox fork, plus policy-as-code checks: no secret exposure, no gate weakening |
| Contract changes | Impact analysis across dependents |

**After merge**
- **Audit sampling.** The auditor re-reviews **10% of merged PRs, and 100% of high-risk ones,** within 24 h, using a different model configuration. Findings become WPs. If the audit disagreement rate goes above 5%, the conductor recalibrates the reviewer prompts.
- **Canary defects.** The conductor periodically seeds known bugs into synthetic PRs to measure each reviewer's catch rate. Rubber-stamp detection uses this, and reviewers below 90% are retired or reconfigured.
- **Optional human spot checks.** A weekly digest of 5 random merged PRs. It never blocks anything.

### 12.9 Human escalation, and how humans are notified
**Escalation triggers.** Only these cases reach a human:

| Trigger | Case |
|---|---|
| E1 | Review deadlock after the protocol in §12.5 is exhausted (3 + 2 rounds plus arbitration). |
| E2 | Main stays red for more than 4 h because automated revert and bisect failed. |
| E3 | A legal or licensing question: a contamination hit not resolved by re-implementation; a dependency licence outside policy; questions about SDK agreements (AAX, ASIO, Unity) or trademarks. |
| E4 | Credentials or external accounts: Apple certificates and notarisation, Avid/PACE, store accounts, DAW licences, an expiring crates.io token. |
| E5 | Budget overrun above 150%, or a WP that fails after re-planning. |
| E6 | A security incident: a critical advisory without a fix, or a suspected secret leak. |
| E7 | A contract change affecting more than 5 in-flight WPs, or a product decision the spec room cannot settle from docs or observation. |

**Notification mechanism**
- The conductor opens an issue labelled `needs-human` with a JSON summary, the options and a recommended default.
- It posts the issue to the configured webhook: Slack, Teams, email or a pager, by severity.
- Open escalations go into a daily digest.
- Humans answer with structured commands in the issue (`/decide 2`, `/approve-licence`, `/rotate-token done`). The conductor parses the answer and resumes.
- The rest of the pipeline keeps running in the meantime. Only the affected WPs park.
- Each severity has an SLA. When it expires, the conductor takes the safe default: park the WP, keep the revert, or block the dependency.

### 12.10 Releases (automated)
- **Versioning.** Crates use semver. Contract crates are released in lockstep, while implementation crates are versioned independently with release-plz (conventional commits plus `changes/*.json`). The project stays on 0.x until the Phase 7 exit.
- **Release gates.**
  - cargo-semver-checks against the last release;
  - full CI and nightly suites green within the last 24 h;
  - the ledger has no `done → partial` regressions;
  - the agent-eval KPI is at or above its threshold;
  - docs versions generated;
  - SBOM (cargo-about, cargo-auditable) and cargo-vet complete.
- **Publishing.** The PR manager publishes to crates.io in dependency order using CI-held tokens. crates.io trusted publishing via OIDC is preferred if available; its availability is UNVERIFIED in this research.
- **Changelogs and tags.** git-cliff generates the changelog. Tags and GitHub releases are created automatically, and binaries (the `cargo-syrp` CLI, the MCP server) are built with cargo-dist.
- **Rollback.** An automatic yank happens only on a critical regression detected within 48 h, and it is always followed by a patch release.

### 12.11 Failure modes and mitigations
| Failure mode | Mitigation |
|---|---|
| Style drift across 100+ crates | rustfmt and clippy configuration; `xtask style` lints; exemplar "golden crates" referenced in AGENTS.md; a reviewer checklist item; a quarterly consistency-sweep WP |
| Duplicated utilities | The utility registry in the API index; `docs search` mandated before writing helpers; a repo-internal clone detector as a G9 sub-check; utilities only in foundation crates, via change request |
| Rubber-stamp reviews | Reviewers rerun everything and attach logs; canary defects; audit sampling; adversarial reviewers from a different model family; reviewer KPIs |
| Merge conflicts in shared files | The PR manager is the sole editor; globbed workspace members; generated files regenerated in the merge queue; batched change requests |
| Hallucinated APIs | The compile and test loop; MCP and CLI docs queries against the real API index; `cargo syrp check-refs` validates that every symbol cited in docs and specs exists; reviewer verification |
| Drift from JUCE semantics | Behaviour specs plus mandatory differential scenarios for each WP; ledger evidence comes only from CI; nightly upstream-delta jobs; parity sweeps at phase exit |
| Flaky CI | Deterministic ManualLoop and seeded RNG; hermetic images; nextest flake detection; quarantine plus auto-fix WPs; device-lab tests are nightly-only, never merge-gating |
| Contract churn | Frozen snapshots; contract-change WPs with impact analysis; escalation above 5 affected WPs |
| Stalled or looping agents | Leases, heartbeats, loop detection, fresh restarts with failure summaries, re-planning |
| Spec ambiguity | A Q&A thread on each WP issue answered by the spec room, preferably by adding an observation scenario |
| Oversized PRs | A 1,200-line cap enforced by CI; the PR manager asks for a split |
| Clean-room contamination | §10.9 scanner and room separation; re-implementation by a different agent on a hit |
| Cost blow-ups | Budgets per WP, wave and phase; throttling; E5 |

## 13. Documentation and closed-loop agent tooling

### 13.1 One source, two documentation sites
**Single sources.** Nothing in the output is written by hand.
- Rustdoc comments, for the API.
- `docs/` Markdown for mdBook: guides, tutorials, cookbook, migration and design (ADRs).
- Structured data:
  - `errors/*.toml`;
  - `parity/ledger.json`;
  - migration entries, as front matter in `docs/migration/*.md`;
  - recipes, as front matter plus code in `examples/recipes/`;
  - per-crate `AGENTS.md`.
- `syrp-docgen` renders both sides from these sources in CI. Generated files carry a `GENERATED` header and are never hand-edited. A freshness gate fails CI if the regenerated output differs.

**Human side** (placeholder domain `docs.syrp.dev`):

| Area | Content |
|---|---|
| Getting started | Build a gain plugin, a synth or a standalone app |
| Guides | Threading and RT; the message loop; components and LookAndFeel; parameters and state; graphs; plugin formats and bundling; platforms |
| Cookbook | Short tested recipes |
| Migrating from JUCE | Concept guides (MessageManager → MessageLoop, APVTS → ParamTree, Component → Component) and a page per class, generated from the migration map |
| API reference | rustdoc |
| Design rationale | ADRs |
| Parity dashboard | Rendered from the ledger |
| Changelog | Breaking changes with codemods |

**Agent side** (`/agents/`):
- **`llms.txt`**, plus **`llms-full.txt`** and per-crate slices `llms-<crate>.txt` sized to context budgets (llmstxt.org layout).
- **`.md` mirrors** of every page.
- **`api-index.json`**, with a JSON Schema. Built from rustdoc JSON. Producing it locally needs unstable rustdoc flags; whether that is still nightly-only on 1.99 is UNVERIFIED. docs.rs also serves it: https://docs.rs/about/rustdoc-json.
- **`ledger.json`** and **`capabilities.json`**.
- **Per-crate agent briefs** of at most about 1,500 tokens: purpose, key types, invariants, thread rules, the top tasks with snippets, error codes and gotchas.
- **Task recipes** in structured form: goal, preconditions, steps, commands, verification, common errors.
- **`errors.json`** (the error catalogue with fixes) and **`migration-map.json`** (JUCE class and method → Rust item, pattern and pitfalls).
- **CLI and MCP JSON Schemas**, and the downloadable Agent Skill.

**Toolchain and versioning**
- mdBook 0.5 with `mdbook test`, rustdoc (HTML and JSON), rustdoc-types, schemars, lychee for link checking, and a static search index.
- Docs are versioned per release (`/v0.7/`, `/latest/`, `/dev/`). Each release snapshot is immutable, and each has its own llms.txt and api-index. API-index entries carry `since` and `deprecated`.

**CI checks**
- `cargo test --doc` and `RUSTDOCFLAGS="-D warnings" cargo doc`.
- The missing-docs and example lints.
- `mdbook test`, plus the recipe runner, which executes each recipe's verification commands.
- lychee link check.
- Docgen freshness.
- Brief token budgets, and a structural validation of llms.txt.
- `check-refs`, which fails if any doc or spec cites a non-existent item.

### 13.2 The CLI: `cargo syrp`
**Output and errors**
- Output is JSON by default when stdout is not a TTY, or with `--json`.
- There are no interactive prompts. Every command has a published JSON Schema.
- Every result uses one envelope: `{schema, command, ok, exit_code, duration_ms, results[], diagnostics[{code, severity, message, location, hint, docs}], artifacts[]}`.

**Exit codes** (stable across versions):

| Code | Meaning |
|---|---|
| 0 | OK |
| 1 | Checks or tests failed |
| 2 | Usage error |
| 3 | Environment or toolchain missing |
| 4 | Parity regression |
| 5 | Validator failure |
| 6 | Golden mismatch |
| 7 | Contract or API violation |
| 8 | RT-safety violation |
| 9 | Clean-room or licence gate failure |
| 10 | Internal error |

**Commands**

| Command | What it does |
|---|---|
| `new <app\|plugin\|console\|lib> [--formats clap,vst3,au,lv2,aax]` | Scaffolds a project with `Syrp.toml`, tests, goldens and AGENTS.md |
| `add <module\|param\|widget\|format>` | Adds a feature, parameter, widget or plugin format, with test stubs |
| `doctor` | Diagnoses the toolchain, SDK paths (ASIO, AAX), signing identities, devices and validators |
| `build [--format …] [--target …]` | Builds, with diagnostics mapped to SYRP codes |
| `check` | The fast gate: fmt, clippy, layer and size lints, contract diff, missing docs |
| `test [--affected] [--crate X]` | nextest, doctests and compile-fail tests, with structured results |
| `render-audio <processor\|graph\|plugin> --input --params --out` | Deterministic offline render to WAV plus JSON statistics |
| `render-ui <component\|editor\|demo> --scale --state --out` | Headless PNG render on the CPU renderer |
| `snapshot diff\|list\|request-update` | Compares against goldens. Updates only become a `golden-update` request. |
| `parity status\|run <scenario>\|check [--wp]\|diff` | Queries the ledger; runs scenarios against references |
| `validate <bundle> [--pluginval\|--clap\|--vst3\|--auval\|--lv2lint\|--aax\|--all]` | Runs the plugin validators and normalises their results |
| `host <plugin> [--script automation.json]` | A headless fake DAW: load, automate, render, dump state |
| `inspect tree\|a11y\|params\|graph\|state\|devices [--attach <pid>]` | JSON introspection |
| `bundle`, `sign`, `notarize` | Builds bundles and signs and notarises them (credentials from the environment) |
| `rt-check` | Allocation-guard and RTSan suites |
| `bench [--against baseline\|oracle]` | Benchmarks against baselines or oracle numbers |
| `fuzz <target> [--time]` | Fuzzing with corpus management |
| `docs search\|show\|brief\|recipe` | Offline queries of the API index, briefs and recipes |
| `explain <SYRP-code>` | Cause, fix and example for an error code |
| `migrate lookup <juce::Symbol>\|plan <juce-project>\|apply` | JUCE → Syrp mapping and codemods |
| `expand <item>` | Shows code generated by derives or `build.rs` |
| `license attest\|report` | Local self-attestation; dependency licence report |
| `wp show\|claim\|check\|submit [--blocked]` | Work-package helpers (project agents only) |
| `review <PR>` | Reviewer harness that emits the checklist JSON (project agents only) |
| `mcp serve [--stdio\|--http]` | Starts the MCP server |
| `run --device`, `package` | Mobile and web (Phase 6) |
| `bindings gen\|check` | Bindings (Phase 8) |

### 13.3 Agent Skill and MCP server
- **One implementation, three front-ends.** The CLI, the MCP server and the skill's scripts all call the `syrp-cli` library. Results and schemas are therefore identical everywhere.
- **Same tools for everyone.** Implementer, reviewer and PR-manager agents use them, and so do downstream users' agents. Project-only commands are hidden from the public tool list by a feature flag.

**MCP server (`syrp-mcp`)**
- Built on rmcp 3.7, targeting MCP 2026-07-28.
- **Tools:**

| Group | Tools |
|---|---|
| Project | `scaffold`, `add`, `doctor` |
| Build and test | `build`, `check`, `test` |
| Rendering and snapshots | `render_audio`, `render_ui` (returns an image resource), `snapshot_diff` |
| Parity and validation | `parity_status`, `parity_check`, `validate_plugin` |
| Hosting and inspection | `host_script`, `inspect` |
| Docs and migration | `docs_search`, `docs_get`, `explain`, `migrate_lookup` |
| Project-internal | `wp_*`, `review` |

- **Resources:** `syrp://api-index`, `syrp://ledger`, `syrp://capabilities`, `syrp://brief/{crate}`, `syrp://recipe/{id}`, `syrp://errors/{code}`, `syrp://migration/{juce_symbol}`.
- Long builds, renders and validator runs use the **Tasks** extension.

**Agent Skill sketch** (`skills/syrp/`, Agent Skills format):

```markdown
---
name: syrp
description: Build, port, test and verify audio apps and plugins with the Syrp Rust framework
  (a JUCE-equivalent). Use when writing JUCE-style code in Rust, porting JUCE projects, creating
  CLAP/VST3/AU/LV2/AAX plugins, DSP or GUIs, or when a `cargo syrp` command or SYRP-* error appears.
---
### The Syrp closed loop
1. Run `cargo syrp doctor --json` once per session; fix anything it reports first.
2. Before coding, look things up instead of guessing:
   `cargo syrp docs search "<concept>"`, `cargo syrp migrate lookup juce::<Symbol>`,
   `cargo syrp docs brief <crate>`.
3. After every edit: `cargo syrp check --json`, then `cargo syrp test --affected --json`.
4. Audio work: `render-audio` and `parity check`. UI work: `render-ui` and `snapshot diff`.
   Plugins: `bundle` and `validate --all`.
5. On any SYRP-* code, run `cargo syrp explain <code>`, apply the fix, and rerun. Never suppress it.
#### Hard rules
- Never allocate, lock or do I/O inside `process`. If you need memory, take it in `prepare`.
- Message-thread APIs need a `MessageThread` token. Use `post()` from other threads.
- Never edit goldens, `contracts/`, ledger evidence or shared files. Request changes instead.
- Clean room: never fetch or recall JUCE source. Work from specs, docs and scenarios.
#### Recipes (references/recipes/)
new-plugin-with-params.md · port-juce-audioprocessor.md · add-undoable-property.md ·
fix-failing-golden.md · add-audio-format.md · webview-editor.md · host-a-plugin.md
#### References
references/api-index.slim.json · references/errors.json · references/migration-map.json ·
references/capabilities.json
#### Scripts
scripts/loop.sh   # runs check → test → (render|parity) and prints one JSON summary
```

**Distribution.** The skill is versioned with each release and published at `/agents/skill.zip`. It is also built into `cargo syrp new` projects (`.agents/skills/syrp`) and comes in per-language variants in Phase 8.

### 13.4 How the tooling drives the §12 loops
**Implementer loop**
1. `wp show <id>` loads the spec, contracts, scenarios and briefs.
2. Turn the scenarios into failing tests first: `parity run` reports red.
3. Implement, iterating `check → test --affected → parity check --wp → render-*/snapshot diff → rt-check` until everything is green.
4. `wp check <id>` runs the full acceptance command, which is identical to the reviewer's.
5. `wp submit` opens a PR with the template front matter filled from JSON.
6. If stuck, `wp submit --blocked` sends a question to the spec room.

**Reviewer loop**
1. `review <PR>` checks out the PR in a fresh sandbox.
2. It reruns `wp check`, the WP scenarios plus a random 10% sample of others, and `snapshot diff` with images attached. It also runs `validate` for wrappers.
3. It evaluates the checklist and posts the structured review.
4. The adversarial verifier additionally writes attack tests: trybuild misuse cases, proptest and fuzz seeds, and threading stress.

**PR-manager loop**
1. Validate the PR front matter.
2. Order PRs by the WP graph, rebase them, and enqueue.
3. Triage failures with `test`/`parity` plus bisection, and quarantine flakes.
4. Run `parity status --update-from-ci` and `docs build`.
5. Compile release notes and run the release pipeline (§12.10).

**Downstream users' agents** use the same loop without the `wp` and `review` commands.

### 13.5 When it gets built
The tooling **lands in Phase 0** because every later agent depends on it (§7.2).

| When | What lands |
|---|---|
| Wave 0.1 | CLI skeleton (envelope, exit codes, `doctor`, `explain`, `wp`), testkit v0, docs pipeline v0 (mdBook, rustdoc JSON → api-index, llms.txt), ledger schema |
| Wave 0.2 | MCP server v0, Agent Skill v0, docgen v1 (briefs, migration-map scaffold, error pages), agent-eval harness |
| Later phases | Commands added per phase, as listed in the tooling table at the end of §7.2 |

**Bootstrapping.** The first few Phase 0 WPs (repo scaffolding, CLI skeleton) are reviewed with plain `cargo` commands and a checklist run by the agents themselves. From the moment `wp check` and `review` exist, every WP goes through the full closed loop. The tooling is dogfooded on itself from then on.
