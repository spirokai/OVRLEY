# Keyboard Shortcut Candidates

`Mod` means `Ctrl` on Windows/Linux and `Command` on macOS.

## Existing Shortcuts - Must Preserve

| Context                | Shortcut                  | Current action                                |
| ---------------------- | ------------------------- | --------------------------------------------- |
| Playback               | `Space`                   | Play/pause                                    |
| Playback               | `Left Arrow`              | Step backward 1 second                        |
| Playback               | `Right Arrow`             | Step forward 1 second                         |
| Widget editing         | `Delete`                  | Delete selected widgets                       |
| Widget editing         | `Mod+C`                   | Copy selected widgets                         |
| Widget editing         | `Mod+V`                   | Paste copied widgets                          |
| History                | `Mod+Z`                   | Undo                                          |
| History                | `Mod+Shift+Z`             | Redo                                          |
| History                | `Ctrl+Y`                  | Redo on Windows                               |
| Selected timeline clip | `Left Arrow`              | Nudge synchronization backward by 0.1 seconds |
| Selected timeline clip | `Right Arrow`             | Nudge synchronization forward by 0.1 seconds  |
| Widget drawer          | `Esc`                     | Close widget drawer                           |
| Dialog                 | `Esc`                     | Close the active dismissible dialog           |
| Deferred input         | `Enter`                   | Commit the current value                      |
| Numeric input          | `Up Arrow` / `Down Arrow` | Step and commit the current value             |

## Proposed Shortcuts

| Category             | Shortcut                                                  | Action                                     | Priority |
| -------------------- | --------------------------------------------------------- | ------------------------------------------ | -------- |
| Template and media   | `Mod+N`                                                   | New template                               | P0       |
| Template and media   | `Mod+O`                                                   | Import/open template                       | P0       |
| Template and media   | `Mod+S`                                                   | Save template                              | P0       |
| Template and media   | `Alt/Option+A`                                            | Import activity                            | P0       |
| Template and media   | `Mod+I`                                                   | Import video                               | P0       |
| Template and media   | `Mod+T`                                                   | Open template selector                     | P1       |
| Template and media   | `Mod+Shift+E`                                             | Open output folder                         | P1       |
| General editing      | `Esc`                                                     | Clear widget selection                     | P0       |
| General editing      | `Backspace`                                               | Delete selected widgets                    | P0       |
| General editing      | `Arrow keys`                                              | Nudge selected widgets by 1 canvas pixel   | P0       |
| General editing      | `Shift+Arrow keys`                                        | Nudge selected widgets by 10 canvas pixels | P0       |
| General editing      | `N`                                                       | Toggle snapping                            | P1       |
| General editing      | `G`                                                       | Toggle grid                                | P1       |
| General editing      | `Mod++` / `Mod+-`                                         | Canvas zoom in/out                         | P1       |
| General editing      | `Mod+0`                                                   | Reset canvas to 100%                       | P1       |
| Playback             | `Home / Fn+left arrow`                                    | Go to timeline start                       | P0       |
| Playback             | `End / Fn+right arrow`                                    | Go to timeline end                         | P0       |
| Playback             | `M`                                                       | Mute/unmute video                          | P1       |
| Export range         | `I`                                                       | Set export In point at playhead            | P0       |
| Export range         | `O`                                                       | Set export Out point at playhead           | P0       |
| Export range         | `Mod+X`                                                   | Clear In and Out points                    | P0       |
| Export range         | `Alt/Option+I`                                            | Playhead to In point                       | P1       |
| Export range         | `Alt/Option+O`                                            | Playhead to Out point                      | P1       |
| Timeline             | `Shift+Z`                                                 | Fit complete timeline                      | P0       |
| Timeline             | `Alt/Option+1`                                            | Fit all media                              | P1       |
| Timeline             | `Alt/Option+2`                                            | Fit activity                               | P1       |
| Timeline             | `Alt/Option+3`                                            | Fit video                                  | P1       |
| Synchronization      | `Shift+Left Arrow` / `Shift+Right Arrow` on selected clip | Nudge synchronization by one second        | P0       |
| Synchronization      | `[` / `]`                                                 | Nudge synchronization globally by -1/+1 second | P0    |
| Synchronization      | `Mod+Shift+A`                                             | Auto-sync activity and video               | P1       |
| Export               | `Mod+E`                                                   | Open Render settings                       | P0       |
| Panels and workspace | `Alt/Option+W`                                            | Toggle widget drawer                       | P1       |

## Implementation Plan

### Scope

- Implement all shortcuts in the proposed table.
- Do not implement export-range playback with `Shift+Space` in this stage.
- Do not add a separate fit-canvas shortcut. `Mod+0` remains the only canvas reset command.
- Preserve every shortcut in the existing-shortcuts table.

### Keyboard Ownership

Keep execution centralized by domain instead of moving all keyboard behavior into one global hook.

| Owner | Responsibilities |
| --- | --- |
| New app-shell keyboard hook | Template/media commands, output folder, Render dialog, template selector, auto-sync, widget drawer |
| Existing `useEditorKeyboard` | Backspace deletion, widget arrow nudging, grid, snapping, canvas zoom/reset |
| Existing `useOverlayPlayer` keyboard handling | Playback, In/Out commands, timeline fit commands, mute, global `[`/`]` sync nudging |
| Existing `useTimelineClips` handlers | Selected-clip arrow nudging, including the new one-second Shift variant |
| Existing `useUndoRedo` | Undo and redo without behavioral changes |
| Existing controls | Dialog Escape and input Enter/arrow behavior without behavioral changes |

Do not introduce a keyboard provider, event bus, imperative child refs, or global execution registry in this stage. Those would require lifting canvas and timeline state out of their owning features.

### Listener Lifecycle

Migrate only the global keyboard listeners touched by this work from dependency-driven listener reattachment to React 19 `useEffectEvent`.

Apply the pattern to:

- The new app-shell keyboard hook.
- `useEditorKeyboard`.
- The global keyboard listener in `useOverlayPlayer`.
- The global keyboard listener in `useUndoRedo`.
- `WidgetDrawer` Escape handling.

The stable effect owns registration and cleanup, while the Effect Event reads current state and callbacks:

```js
const onKeyDown = useEffectEvent((event) => {
  // Match and execute using current state and callbacks.
})

useEffect(() => {
  const handleKeyDown = (event) => onKeyDown(event)

  window.addEventListener('keydown', handleKeyDown)
  return () => window.removeEventListener('keydown', handleKeyDown)
}, [])
```

For `WidgetDrawer`, registration may remain conditional on `widgetDrawerOpen`; `useEffectEvent` keeps the close command current while the effect attaches on open and cleans up on close.

Do not migrate unrelated effects as part of this work. Element-level handlers such as `useTimelineClips` remain ordinary `onKeyDown` and `onKeyUp` props because React already owns their lifecycle.

Every manually registered listener must:

- Return cleanup that removes the exact function that was registered.
- Use the same capture option for registration and removal.
- Avoid anonymous add/remove function pairs that cannot match.
- Remain safe when `window` is unavailable in tests.

### Shortcut Manifest

Move the manifest to `app/src/data/keyboardShortcuts.json` and use it as the single source of truth for app-defined shortcut metadata and bindings. Production handlers and the help modal must not hard-code their own key combinations.

Each manifest command should declare:

- A stable command ID, such as `template.new`, `editor.delete`, or `player.toggleMute`.
- Its owning scope: `app`, `editor`, `player`, `timeline-clip`, `history`, or `drawer`.
- Help category and user-facing description.
- One or more bindings using normalized modifiers such as `mod`, `ctrl`, `shift`, and `alt`.
- Optional platform restrictions or display aliases where needed, such as `Home / Fn+Left Arrow`.

Example shape:

```json
{
  "id": "template.new",
  "scope": "app",
  "category": "Template and media",
  "description": "New template",
  "bindings": [{ "key": "n", "modifiers": ["mod"] }]
}
```

Add `app/src/lib/keyboard-shortcuts.js`. It imports and validates the manifest once, then exports:

- `matchKeyboardShortcut(event, scope)`, returning the matching command ID and binding metadata.
- The validated shortcut manifest for consumers that need to render its metadata.

Validation must fail loudly for malformed entries, unknown modifiers, duplicate command IDs, or duplicate bindings within the same scope. Bindings use exact modifier matching so, for example, `Alt+A` does not also match `Ctrl+Alt+A` from an AltGr keyboard.

Feature handlers remain responsible only for behavior:

```js
const match = matchKeyboardShortcut(event, 'player')

switch (match?.commandId) {
  case 'player.toggleMute':
    toggleVideoMute()
    break
}
```

Apply the matcher to the existing app-owned handlers as part of this work:

- `useUndoRedo`
- `useEditorKeyboard`
- `useOverlayPlayer`
- `useTimelineClips`
- `WidgetDrawer` Escape handling
- The new app-shell keyboard hook

Help grouping and friendly key-label formatting are keyboard-dialog concerns, not shared keyboard-matching behavior. Keep that transformation in a small utility within the keyboard-dialog feature, such as `app/src/features/app-shell/utils/keyboardShortcutGroups.js`. `KeyboardShortcutsDialog` consumes the grouped result and remains presentational. The utility consumes the validated manifest exported by `lib/keyboard-shortcuts.js`; it must not import a second display-only shortcut list. Special hardware aliases such as `Fn+Left Arrow` may be declared in the same manifest entry.

Native control behavior such as Radix dialog Escape, input Enter, and numeric-input arrows remains owned by those controls. It is preserved and tested but does not need a manifest command because the application does not register those bindings.

### App-Shell Commands

Add one app-shell keyboard hook and call it from `useAppShellComposition` in `App.jsx`. Pass the callbacks already produced by the shell hooks:

| Shortcut | Existing command |
| --- | --- |
| `Mod+N` | `templateManagement.handleCreateNewTemplate` |
| `Mod+O` | `templateManagement.handleImportTemplate` |
| `Mod+S` | `templateManagement.handleSaveTemplate` |
| `Alt/Option+A` | `handleActivityFileOpen` |
| `Mod+I` | `videoControls.handleImportVideo` |
| `Mod+T` | New template-selector open command owned by template management |
| `Mod+Shift+E` | `handleOpenDownloads` |
| `Mod+Shift+A` | `computeVideoSync(activitySummary)` |
| `Mod+E` | `renderWorkflow.openRenderDialog` |
| `Alt/Option+W` | Store `toggleWidgetDrawer` action |

Commands must honor the same availability rules as their buttons. Disabled UI actions must remain no-ops when invoked by keyboard.

### Auto-Sync

The Auto-sync button currently receives `onComputeVideoSync`, which resolves to `computeVideoSync(activitySummary)` through `useSceneSettingsState`.

The shortcut should invoke that same store command directly. It should not synthesize a click or duplicate synchronization calculations. The command already updates `videoSyncOffsetSeconds`, `videoSyncWarning`, and `videoSyncTimezoneMode`, so the button and shortcut will produce the same result.

The shortcut is available only when both an imported video and an activity summary exist.

### Template Selector

Control only the template selector's Radix `Select`; do not add selector-open state to Zustand.

- Add local React state to `useTemplateManagement` for whether the template selector is open.
- Expose `openTemplateSelector`, `templateSelectorOpen`, and `setTemplateSelectorOpen` from that hook.
- Pass `open` and `onOpenChange` through `templateControls` to `TemplateSection`.
- Bind `Mod+T` to `openTemplateSelector` in the app-shell keyboard hook.
- Selecting a template or dismissing the Select closes it through `onOpenChange`.

This keeps `TemplateSection` presentational and controls only that specific Select.

### Canvas Commands

Extend `useEditorKeyboard` rather than moving canvas behavior into the app shell.

- `Esc` clears widget selection only when no dialog, popover, Select, or widget drawer is open.
- `Backspace` uses the same deletion command as `Delete`.
- Arrow keys update all selected widgets by one scene pixel.
- Shift+Arrow updates all selected widgets by ten scene pixels.
- `N` toggles `editorSnapToGrid`.
- `G` toggles `editorGridVisible`.
- `Mod++` and `Mod+-` call the existing zoom commands.
- `Mod+0` calls the existing reset-zoom command.

Canvas arrow handling must run before global playback arrow handling and call `preventDefault`. It must remain disabled for interactive elements and focused timeline clips.

### Player And Timeline Commands

Extend the keyboard handling owned by `useOverlayPlayer`, where the required playback, export-range, and viewport commands already exist.

| Shortcut | Command |
| --- | --- |
| `Home` | `playback.resetToStart` |
| `End` | `playback.jumpToEnd` |
| `M` | `toggleVideoMute` |
| `I` | `exportTimeline.setBoundary('from', exportBoundarySecond)` |
| `O` | `exportTimeline.setBoundary('to', exportBoundarySecond)` |
| `Mod+X` | `exportTimeline.clear` |
| `Alt/Option+I` | Move the playhead to the custom range start |
| `Alt/Option+O` | Move the playhead to the custom range end |
| `Shift+Z` | Fit the complete timeline |
| `Alt/Option+1` | Fit target `all` |
| `Alt/Option+2` | Fit target `activity` |
| `Alt/Option+3` | Fit target `video` |

`Alt/Option+I` and `Alt/Option+O` require `useExportRangeTimeline` to expose its current committed range boundaries. They do not require new playback behavior.

### Synchronization Nudging

Selected timeline clips retain their current arrow-key behavior:

- Left/Right nudges by 0.1 seconds.
- Shift+Left/Right nudges by 1 second.
- The nudge commits on keyup or blur as it does today.

Global bracket shortcuts do not require selecting a clip:

- `[` subtracts 1 second from `videoSyncOffsetSeconds`.
- `]` adds 1 second to `videoSyncOffsetSeconds`.
- They are available only when video and activity data both exist.
- They commit immediately through the canonical `setVideoSyncOffset` action.
- Invalid offsets retain the previous value and update `videoSyncWarning`, matching the existing sync controls.
- Ignore key repeat so a held key does not flood undo history.

### Event Guards And Priority

Use this handling order:

1. Active dialog, Select, popover, or drawer
2. Focused input or interactive control
3. Focused timeline clip
4. Canvas selection commands
5. Player and timeline commands
6. App-shell commands

Every handler must return when `event.defaultPrevented` is true. Text and numeric inputs retain their native copy, paste, selection, character entry, Enter, and arrow behavior.

### Shortcut Documentation

Update `app/src/features/app-shell/data/keyboardShortcuts.json` after implementation so it includes both preserved and new shortcuts. Tooltips and `aria-keyshortcuts` should be updated for controls that expose a corresponding command.

### Verification

Add focused tests for:

- App-shell shortcut matching, availability, and input suppression.
- Existing `Mod+V` widget paste remaining intact after video import moves to `Mod+I`.
- Canvas arrows taking priority over playback arrows when widgets are selected.
- Selected-clip arrows taking priority over canvas and playback handling.
- In/Out, fit, Home/End, mute, and global bracket commands.
- Auto-sync shortcut producing the same store transition as the Auto-sync button command.
- Template selector opening with `Mod+T` and closing normally.
- Existing undo/redo, dialog Escape, drawer Escape, and input keyboard behavior remaining unchanged.
