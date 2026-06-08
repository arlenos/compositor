# Porting Notes: cosmic-comp Fork

This document describes all changes made to the cosmic-comp upstream codebase,
the rationale behind each, and the process for keeping the fork in sync with upstream.

## Upstream

- **Source:** https://github.com/pop-os/cosmic-comp
- **Tracked as:** `git remote add upstream https://github.com/pop-os/cosmic-comp`
- **Branch strategy:** upstream is **merged into** `master` (not rebased onto). A
  long-lived fork has many of its own commits; rebasing replays all of them on
  every sync and the same conflicts resurface. Merging resolves only the genuine
  delta once and keeps our history intact. `git log --oneline upstream/master..HEAD`
  still shows our delta against upstream at any time.

## What we keep

Everything related to the Wayland compositor core:

- `src/backend/` — DRM/KMS, rendering, Winit and X11 backends
- `src/shell/layout/` — floating and tiling layout engine (the primary reason for this fork)
- `src/shell/element/` — window elements, stack (Iced dependency removed; all UI delegated to desktop-shell via protocol)
- `src/shell/workspace.rs` — workspace management
- `src/shell/focus/` — keyboard focus management
- `src/input/` — input handling, gestures
- `src/wayland/handlers/` — standard Wayland protocol handlers
- `src/xwayland.rs` — XWayland integration

## What we add

### `src/event_bus.rs`

Non-blocking Event Bus integration. Opens a Unix socket connection to the Arlen
Event Bus in a background thread and emits structured events without touching the
compositor render loop.

**Events emitted:**

| Event type | Trigger | Source field |
|---|---|---|
| `window.opened` | Surface successfully mapped to workspace | `wayland` |
| `window.focused` | Keyboard focus changes | `wayland` |
| `window.closed` | Toplevel surface destroyed | `wayland` |
| `clipboard.copy` | Selection changes on clipboard target | `wayland` |

The socket path is read from `ARLEN_PRODUCER_SOCKET` (default:
`/run/arlen/event-bus-producer.sock`). The session ID is read from
`ARLEN_SESSION_ID` (default: a fresh UUID v7 generated at startup).

**Design constraint:** All Event Bus calls are non-blocking. The compositor event
loop must never stall waiting for I/O. The background thread absorbs all socket
writes; if the channel is full, events are dropped with a warning rather than
blocking.

### Changes to `src/state.rs`

Added `event_bus: crate::event_bus::EventBusHandle` field to `Common`. Initialized
in the `Common` constructor via `crate::event_bus::spawn()`.

### Changes to `src/shell/focus/mod.rs`

Added `emit_window_focused` call in `Shell::set_focus` after the focus stack is
updated, before `update_focus_state`.

### Changes to `src/wayland/handlers/compositor.rs`

Added `emit_window_opened` call after successful `shell.map_window` in the Wayland
compositor handler.

### Changes to `src/xwayland.rs`

Added `emit_window_opened` call after successful `shell.map_window` in the XWayland
map notify handler.

### Changes to `src/wayland/handlers/xdg_shell/mod.rs`

Added `emit_window_closed` call at the start of `toplevel_destroyed`, before the
surface is unmapped (so the app ID is still available).

### Changes to `src/wayland/handlers/selection.rs`

Added `emit_clipboard_copy` call at the start of `new_selection` for
`SelectionTarget::Clipboard`. The MIME type is included; clipboard content is never
logged.

### Changes to `Cargo.toml`

Added `prost` and `uuid` as dependencies, `prost-build` as a build dependency.

### Changes to `build.rs`

Added `prost_build::compile_protos` call to generate Rust bindings from
`proto/event.proto`.

### New file: `proto/event.proto`

Copy of the Event Bus protobuf schema from the `event-bus` repository. Must be kept
in sync with `event-bus/proto/event.proto`. A shared `proto` crate is planned for
Phase 2 to eliminate this duplication.

### Exclusive zone fix for 4-anchor layer surfaces

`src/wayland/handlers/compositor.rs` -- Before `layer_map_for_output().arrange()` is
called, we inject `exclusive_edge = Some(Anchor::TOP)` into the cached state of any
layer surface that has all 4 anchors set and an exclusive zone but no explicit edge.
Smithay's `implied_exclusive_edge_for_anchor()` returns `None` for 4-anchor surfaces
(by design: it cannot infer which edge to reserve), which silently disables the
exclusive zone. The desktop-shell uses all 4 anchors for its full-screen overlay
surface with a 36px top bar. Without this fix, windows slide under the bar.

### arlen-shell-overlay protocol: tab bar and indicator events

`resources/protocols/arlen-shell-overlay.xml` -- Extended with tab bar events
(`tab_bar_show`, `tab_bar_hide`, `tab_added`, `tab_removed`, `tab_activated`,
`tab_title_changed`, `tab_activate` request), indicator events (`indicator_show`,
`indicator_hide` with `indicator_kind` enum for stack hover, swap, and resize), and
supporting enums. The protocol allows the compositor to delegate rendering of window
stack tab bars and visual indicators to the desktop-shell process.

`src/wayland/protocols/shell_overlay.rs` -- Added `send_tab_*` and
`send_indicator_show/hide` methods on `ShellOverlayState`, plus the `tab_activate`
handler trait method and dispatch match arm.

`src/wayland/handlers/shell_overlay.rs` -- Implemented `tab_activate`: finds the
stack by `stack_id`, calls `set_active()` on the matching surface.

`src/shell/element/stack.rs` -- Added `stack_id: u32` field with static atomic
counter to `CosmicStackInternal`. Protocol events are emitted from `add_window()`,
`remove_window()`, `remove_idx()`, and `set_active()` via `loop_handle.insert_idle`.
Initial tab list is sent from `CosmicStack::new()`.

`src/wayland/handlers/compositor.rs` -- Added title change detection in the commit
handler: compares current title against a `CachedTitle` in the surface's user data
and emits `tab_title_changed` when it differs.

`src/shell/mod.rs` -- Indicator show events in `set_overview_mode()` (swap,
kind=2) and `set_resize_mode()` (resize, kind=3). Hide events via
`pending_indicator_hides` drained in `Common::refresh()`.

`src/shell/grabs/moving.rs` -- Stack hover indicator show/hide in the move grab
motion handler and both `unset()` implementations.

`src/shell/layout/tiling/mod.rs` -- Resize edge change events emitted when
`possible_edges` differs from the cached value.

### Iced rendering removal (Phase 3 partial)

`src/shell/element/resize_indicator.rs`, `stack_hover.rs`, `swap_indicator.rs` --
`view()` returns an empty row. All Iced widget imports removed. The underlying
`IcedElement` wrapper and `Program` trait implementation remain because they provide
the `SpaceElement` integration, input handling, and render pipeline that the
compositor relies on. Rendering is now delegated to desktop-shell via the indicator
protocol events above.

`src/shell/element/stack.rs` -- `DefaultDecorations::view()` replaced with a minimal
mouse area that preserves `DragStart` and `Menu` messages without rendering any tab
widgets. `TabMessage` trait, scroll-related message variants, and the `SCROLLABLE_ID`
static removed.

`src/shell/element/stack/tab.rs` and `tabs.rs` -- Deleted. These were the custom
Iced `Widget` implementations for individual tabs and the scrollable tab list. Tab
rendering is now handled by desktop-shell via the tab bar protocol.

The menu Iced fallback path (`src/shell/grabs/menu/`) is intentionally kept because
zoom.rs context menus still use it.

### Config: TOML loader replaces cosmic-config for CosmicComp

`src/config/mod.rs` -- The `com.system76.CosmicComp` cosmic-config store and its
`ConfigWatchSource` are replaced by a TOML file at `~/.config/arlen/compositor.toml`
(configurable via `ARLEN_COMPOSITOR_CONFIG`). A `notify` file watcher on the parent
directory detects changes (including atomic editor renames) and sends them to the
calloop event loop via a channel. The `toml_config_changed()` function reloads the
entire TOML file, compares each field against the previous config, and triggers the
same side effects as the old key-based `config_changed()` handler.

The `cosmic_helper` field on `Config` is retained as a legacy write-back channel
because `zoom.rs` and `input/actions.rs` write config values back to cosmic-config.
This will be removed when zoom.rs is replaced.

CosmicTk, Shortcuts, and WindowRules watchers remain on cosmic-config unchanged.

`cosmic-comp-config/src/lib.rs` -- Added `#[derive(Deserialize, Serialize)]` to
`CosmicCompConfig` so it can be deserialized from TOML directly. The
`CosmicConfigEntry` derive is kept for backward compatibility with the legacy
cosmic-config code paths.

`Cargo.toml` -- Added `toml = "0.8"` and `notify = "7"` dependencies.

### Sandbox identifier change

`src/state.rs` -- `ClientState::not_sandboxed()` previously checked for
`com.system76.CosmicPanel` as the sandbox engine. Changed to
`dev.arlen.desktop-shell` to match the Arlen desktop shell identity.

### IcedElement replacement for CosmicStack and CosmicWindow

`src/shell/element/stack.rs` and `src/shell/element/window.rs` no longer use the
`IcedElement<P>` wrapper. Both are now standalone structs with
`Arc<Mutex<Internal>>` + `LoopHandle`. All Smithay traits (`IsAlive`,
`SpaceElement`, `PointerTarget`, `TouchTarget`, `KeyboardTarget`) are implemented
directly. `CosmicStack` hit-tests the top `TAB_HEIGHT` pixels for DragStart
(left-click) and Menu (right-click) via `handle.insert_idle`. `CosmicWindow`
uses the existing `Focus::under()` geometry to route resize edges and header
actions. No Iced widgets are rendered; tab bars and window headers are delegated
to desktop-shell via the shell overlay protocol.

### IcedElement replacement for indicators

`resize_indicator.rs`, `stack_hover.rs`, and `swap_indicator.rs` are standalone
structs with no-op rendering. They store metadata (edges, direction, size) and
expose the same public API (`resize()`, `output_enter()`, `render_elements()`)
but return empty render element lists. The `Program` trait and Iced `view()`
implementations are removed.

### Window header protocol extension

`arlen-shell-overlay.xml` extended with `window_header_show`,
`window_header_update`, `window_header_hide` events and `window_header_action`
request. The `window_header_action_type` enum covers minimize, maximize, close,
and move. Desktop-shell renders header bars with title, activation state, and
conditional minimize/maximize buttons. The compositor routes header button
clicks back to the appropriate window management action.

### Zoom toolbar protocol replacement

`src/shell/zoom.rs` no longer contains any Iced code. `ZoomProgram`,
`ZoomMessage`, `ZoomElement`, `ZoomFocusTarget`, and all PointerTarget/TouchTarget
implementations for the zoom UI are removed. The viewport logic (focal point,
level animation, output state) is preserved. Zoom activation and level changes
emit `zoom_toolbar_show`, `zoom_toolbar_update`, and `zoom_toolbar_hide`
protocol events. Desktop-shell renders the toolbar with zoom controls; user
actions (increase, decrease, close, set increment, set movement) are sent back
as protocol requests.

### Iced menu fallback removal

The Iced context menu rendering path in `src/shell/grabs/menu/mod.rs` is
removed. `ContextMenu`, its `Program` impl, and `menu/item.rs` (the
`SubmenuItem` Iced widget) are deleted. `MenuGrab` now operates exclusively via
the shell overlay protocol. When no desktop-shell client is connected,
`send_context_menu` returns `None` and the menu simply does not appear rather
than falling back to in-compositor Iced rendering.

### ArlenTheme global and cosmic::Theme removal

`src/theme.rs` contains a `RwLock<Option<ArlenTheme>>` global updated by a
`arlen-theme::ThemeWatcher`. All color, radius, gap, and hint reads across the
compositor use `crate::theme::arlen_theme()` instead of `cosmic::Theme`
methods. The `cosmic::Theme` field has been removed from `Common`, `Shell`,
`CosmicStackInternal`, `CosmicWindowInternal`, `FloatingLayout`,
`TilingLayout`, `Workspaces`, and all constructors that previously accepted it.
The `cosmic::theme::system_preference()` initialization and the cosmic theme
file watcher (ThemeMode, dark/light config) are deleted.

### libcosmic dependency removal

The `libcosmic` crate (which pulls in Iced, iced_tiny_skia, and the full cosmic
widget toolkit) is removed from `Cargo.toml`. `utils/iced/mod.rs` and
`utils/iced/state.rs` are deleted. The `cosmic-config` crate remains as a
dependency because the Shortcuts and WindowRules watchers
(`cosmic_settings_config::shortcuts::context()`) return `cosmic_config::Config`
handles. Removing `cosmic-config` requires replacing the shortcut/window-rule
config infrastructure, which is deferred.

### CosmicTk and icon theme removal

The `CosmicTk` config watcher in `config/mod.rs` (which tracked the COSMIC
toolkit icon theme) is removed. `cosmic::icon_theme::set_default()` calls are
deleted. XWayland falls back to the `"hicolor"` icon theme. A Arlen-native
icon theme system is planned for Phase 4.

## Technical debt

### cosmic-config dependency

`cosmic-config` remains as a direct dependency for two reasons: the Shortcuts
watcher (`cosmic_settings_config::shortcuts::context()` returns a
`cosmic_config::Config`) and the WindowRules watcher. The `cosmic_helper` field
on `Config` is also retained for legacy zoom config write-back. Removing
`cosmic-config` requires replacing the shortcut and window-rule configuration
infrastructure with a Arlen-native TOML-based system.

## Upstream sync process

### Weekly merge (automated)

A scheduled CI job (`.github/workflows/upstream-merge.yml`) runs weekly:

1. `git fetch upstream` and skip if nothing is new.
2. `git merge upstream/master` on a dated branch.
3. On a clean merge it builds (`cargo build --lib`) and runs the **integration
   guard** (below); only a green, guard-passing merge opens a pull request.
4. On conflicts it pushes the half-merged branch (markers committed, so the work
   is not lost) and opens an issue. A build or guard failure opens an issue with
   the reason instead of a PR.

The PR is opened with a PAT / GitHub App token (secret `UPSTREAM_MERGE_TOKEN`)
so the fork's CI actually runs on it; the default `GITHUB_TOKEN` would suppress
it.

### Integration guard (what the merge must not drop)

These are the Arlen seams a merge must preserve; the CI guard checks them and
desktop-shell binds the protocol interfaces by exact name:

- `src/event_bus.rs` emits to `/run/arlen/event-bus-producer.sock`.
- the protocol interfaces `arlen_shell_overlay_v1`, `arlen_titlebar_v1` /
  `arlen_titlebar_manager_v1`, `arlen_window_attachment_v1` /
  `arlen_window_attach_manager_v1` (these must match the desktop-shell XMLs).
- the default theme comes from `arlen_theme::DARK_TOML` / `LIGHT_TOML`.

### Manual merge

```bash
git fetch upstream
git merge upstream/master
# Resolve conflicts if any, then:
git commit
cargo build --lib   # the fork builds with --lib, not bare
```

### Building against arlen-theme (cross-repo dependency)

The compositor git-deps `arlen-theme` from `arlenos/arlen` (the monorepo owns
the theme schema and the bundled default `DARK_TOML` / `LIGHT_TOML`). Because it
is a separate repo, the dependency is fixed by `Cargo.lock` to one commit. When
arlen-theme changes land in the monorepo, refresh and re-verify the pin here:

```bash
cargo update -p arlen-theme
cargo build --locked --lib
```

then commit the updated `Cargo.lock`. The regular PR CI (`ci.yml`) should build
`--locked` so the fork always compiles against the exact arlen-theme revision
desktop-shell was validated with. The weekly upstream-merge job deliberately
does NOT use `--locked` (an upstream merge can legitimately change dependencies,
which would make a locked build fail spuriously).

Bootstrapping note: a freshly cloned compositor cannot build until the
arlen-theme revision it pins is published. After the first arlen push that
carries the theme single-source change, run the `cargo update` above and commit
the lock so the pin resolves.

### Contributing patches upstream

Any change that is not Arlen-specific should be submitted as a pull request to
cosmic-comp upstream. A shorter delta means less to resolve on every merge.
Check before committing whether a change belongs upstream.

### Conflict resolution guidelines

**Conflicts in `src/backend/`:** Upstream changes take priority unless they conflict
with our Event Bus socket setup. Accept upstream.

**Conflicts in `src/shell/layout/`:** Upstream changes take priority. This is the
core we fork for; keep it as close to upstream as possible.

**Conflicts in `src/shell/focus/mod.rs`:** Our `emit_window_focused` call must be
preserved after the focus stack update. Re-apply it after accepting upstream changes.

**Conflicts in `src/wayland/handlers/`:** Review case by case. Our additions are
minimal (single function calls); they are easy to re-apply after upstream changes.

**Conflicts in `src/event_bus.rs`:** This file is entirely ours; upstream will never
touch it. No conflicts expected.
