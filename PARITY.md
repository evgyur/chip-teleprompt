# Capability inventory — v21 → Rust 0.2.0

Baseline: installed `C:\PORTABLE\Chip-Teleprompt\Teleprompter.ps1`, v21 plus user defaults 700×320 / 15 pt. Immutable copy: `baseline/Teleprompter.ps1`.

SHA-256: `ae723d102a3f3f775b731484af2ccaf2a208c0eed932ff1eece2604d786f0605`.

Inventory established by read-only inspection before implementation. Baseline has no automated test suite. All capabilities below are reimplemented; none retired.

| ID | Baseline contract | Rust owner / verification |
| --- | --- | --- |
| W1 | Borderless, topmost, primary-screen top center; 700×320 initial; 620×320 minimum | `main.rs`: window styles, startup/minmax/hit-test; native defaults and corner smoke checks |
| W2 | Sticky top / Ctrl+T resets width700, preserves height, activates | `execute(Snap)`; native resized-height smoke |
| W3 | Drag blank toolbar; resize edges and22px grip | `wndproc`, `execute(Move)`; hit-test smoke |
| T1 | Paste Unicode; resets top and preserves playback | `clipboard_text`, `set_text`; Unicode/Cyrillic replacement smoke; uses Windows clipboard API |
| T2 | No blanks removes whitespace-only lines, trims line ends, preserves indentation/playback | `model::remove_blank_lines`; unit + native command checks |
| T3 | Default text, black stage, white Ubuntu15pt, wrapping | `App::new/reflow/render`; actual GDI font check and visually inspected100/150 snapshots |
| P1 | Space/Start/Pause; elapsed-time pixel scrolling; stops only after last line leaves | `Playback`; unit boundary/time-invariance tests + native Space/tray checks |
| P2 | Speed0..100, nonlinear3..180px/s;28 gives17 | `speed_px`, slider; unit tests + native keyboard bounds |
| P3 | Manual drag pauses; clamps short/long text; Top preserves playback | `Playback::drag_to/reset`; unit + native checks |
| F1 | Font dialog supports family/style/size/color; slider14..96 retains style/color | `execute(FontDialog)`, `set_font_size`; native style/color smoke + external dialog-open/cancel check |
| F2 | F11/Clean hides toolbar; Escape restores; padding/reflow | `command`, `stage`, `render`; native keyboard smoke |
| K1 | Ctrl+V/Ctrl+T/Space/F11/Escape; Tab/ShiftTab and keyboard sliders | `wndproc/focus_key`; native key smoke |
| A1 | Visible author, channel, repository hyperlinks; original license retained | `render`, `execute(Link)`, `LICENSE`; source/visual review |
| X1 | Exit closes app | `execute(Quit)`, `WM_CLOSE/DESTROY`; normal-launch shutdown check |
| N1 | User-added tray icon, show/hide, pause/continue/exit; recovery after Explorer restart | `tray.rs`; native Shell registration/callback/recreation and shutdown checks |
| N2 | User-added generated icon embedded in exe/tray/shortcut | resourceID1,9 ICO sizes16..256; visual inspection and packaged resource readback |

Intentional additions: mouse-wheel manual scrolling; tray hide pauses; single-instance launch. Windows DPI is explicit: UI uses96-DPI logical units and renders per-monitor. Text content is not persisted, matching baseline. Normal Exit still exits; the explicit Tray button hides.

Verification limitation: direct Win32 handler tests cover UI behavior, but cannot substitute for human assessment of perceived scrolling smoothness on every monitor. Snapshot100%/150% checks are rendered samples; other monitor arrangements are untested.

Build hashes, fresh test results, package identity, and installation readback are recorded in the delivered `verification.txt`.
