# MCP: OVRLEY for AI agents

`OVRLEY --mcp` is an MCP server on standard input and output. It drives the OVRLEY window that is open on the same computer, as the person at it would: the window shows every change, and edits land in the same undo history. It opens no window and changes nothing by itself.

## Turn it on

1. Open OVRLEY.
2. Open **Settings** and turn on **Allow agent edits**. It is off until you do, and OVRLEY remembers it.
3. Register the server with your agent, with the command of your package.

## The command

| Package           | Command                                                                        |
| ----------------- | ------------------------------------------------------------------------------ |
| Windows installer | `%LOCALAPPDATA%\OVRLEY\app.exe --mcp` (the installer calls the app `app.exe`)  |
| Windows portable  | `<folder>\OVRLEY.exe --mcp`                                                    |
| Microsoft Store   | `ovrley.exe --mcp` (an app execution alias)                                    |
| macOS             | `/Applications/OVRLEY.app/Contents/MacOS/OVRLEY --mcp`                         |
| AppImage          | `<path>/OVRLEY-linux-<version>.AppImage --mcp` (updates keep the file's path)  |
| Debian package    | `/usr/bin/OVRLEY --mcp`                                                        |
| Flatpak           | `flatpak run cc.ovrley.OVRLEY --mcp`                                           |
| Development build | `<checkout>/target/debug/ovrley --mcp`                                         |

Write the path out in full: agent clients do not expand `%LOCALAPPDATA%` or `~`. An installer that was given another folder has `app.exe` there.

Claude Code:

```sh
claude mcp add ovrley -- /Applications/OVRLEY.app/Contents/MacOS/OVRLEY --mcp
```

Clients that take JSON:

```json
{
  "mcpServers": {
    "ovrley": {
      "command": "/Applications/OVRLEY.app/Contents/MacOS/OVRLEY",
      "args": ["--mcp"]
    }
  }
}
```

With the Flatpak, `command` is `flatpak` and `args` are `["run", "cc.ovrley.OVRLEY", "--mcp"]`. Of your files, the sandbox sees only Documents and Videos, so files named to the tools must lie there.

## While an agent works

- **The badge.** The header shows "Agent connected" while an agent is connected and agent edits are allowed. Turning the setting off refuses every further call at once.
- **Undo.** Most edits are one undo step, for you and for the agent (`history_undo`). A tool whose change is outside the history says so in its description.
- **Questions.** Where OVRLEY would ask you (unsaved changes, an existing output file, an import to confirm), it asks in the window and the agent's call waits for your answer. Several tools take the answer as an argument, and then nothing is asked.
- **Paths** in arguments are absolute.
- **Several windows.** The first window that opened serves agents. When it closes, the others do not take over: open OVRLEY again.

## Errors

| The agent reads                                  | Meaning                                                            |
| ------------------------------------------------ | ------------------------------------------------------------------ |
| OVRLEY is not open. …                            | No window is serving. The server connects again on the next call.  |
| Agent edits are turned off in OVRLEY. …          | "Allow agent edits" is off. Reading is refused too.                |
| OVRLEY is busy. Try again.                       | The window could not take the call at that moment.                 |
| OVRLEY did not answer in time. …                 | The window took over 10 seconds. The change may still be applied.  |
| OVRLEY is closing.                               | The window was closing.                                            |
| The request was cancelled before OVRLEY could answer. | What the call waited for went away without an answer.         |

Malformed arguments and unknown tools are JSON-RPC invalid-params errors.

## Tools

Start with `ovrley_summary`. Each tool's full description and input schema come with `tools/list`.

### Summary and settings

- `ovrley_summary`: the state of the window
- `language_set`: the language
- `unit_system_set`: metric or imperial units; the widgets convert
- `drawer_select_tool`: shows a drawer
- `drawer_set_pinned`: pins or unpins the drawer
- `drawer_dismiss`: hides an unpinned drawer
- `global_default_set`: one global default, applied to the widgets that follow it
- `global_defaults_reset`: every global default back to the app's
- `aspect_ratio_set`: the overlay's aspect ratio
- `scene_size_set`: the overlay's size
- `fps_set`: the render frame rate
- `widget_update_rate_set`: how often widgets update

### Templates

- `template_list`: the bundled and the user's templates
- `template_load`: loads a listed template
- `template_import`: opens a template file
- `template_new`: a new, empty template
- `template_save`: saves the template to a file

### Widgets

- `widget_get`: one widget's data and editable fields
- `widget_add`: adds a widget
- `widget_update`: changes a widget's settings
- `widget_move`: moves a widget
- `widget_resize`: resizes a widget
- `widget_rotate`: rotates a course or image widget
- `widget_delete`: deletes a widget
- `widget_raster_image_set`: an image widget's file
- `font_list`: the fonts a widget can use
- `map_style_list`: the installed map styles

### Activity

- `activity_import`: opens an activity file
- `activity_delete`: removes the activity
- `activity_set_lap_enabled`: includes or excludes a lap
- `activity_trim_set`: trims the activity
- `activity_trim_reset`: removes the trim

### Player

- `playback_play`: plays from the playhead
- `playback_pause`: pauses
- `playhead_seek`: moves the playhead
- `video_mute_set`: mutes or unmutes the preview

### Videos

- `video_import`: imports videos
- `video_get`: one clip with its cuts, segments and joins
- `video_locate`: gives a missing clip its file
- `video_move`: moves a clip on the timeline
- `videos_shift`: moves every clip by the same time
- `video_remove`: removes a clip
- `videos_clear`: removes every clip
- `video_cut`: cuts a clip
- `video_cut_move`: moves a cut
- `video_cut_remove`: removes a cut
- `video_trim`: trims an end of a clip
- `video_segment_set_excluded`: excludes or includes a part of a clip
- `video_segment_set_overlay`: shows or hides the widgets over a part
- `video_join_transition_set`: the transition at one join
- `transition_defaults_set`: the default transition
- `video_sync`: places one clip by its recording time
- `videos_sync`: places every clip by its recording time
- `video_time_source_set`: where the recording time comes from
- `video_timezone_set`: reads the camera's clock as local time or UTC

### Projects

- `project_list`: the projects in a folder
- `project_new`: a new project
- `project_open`: opens a project
- `project_save`: saves the project

### Render

- `render_options`: formats, accelerations and what blocks a render
- `render_start`: starts a render
- `render_progress`: the current or last render, optionally waiting for it
- `render_cancel`: cancels the render

### Sync Doctor

- `video_sync_get`: landmarks, location, events and search results
- `video_sync_landmark_add`: adds a landmark
- `video_sync_landmark_move`: moves a landmark
- `video_sync_landmark_set_type`: changes a landmark's type
- `video_sync_landmark_remove`: deletes a landmark
- `video_sync_landmarks_clear`: deletes every landmark
- `video_sync_location_set`: the landmark's place on the course
- `video_sync_location_clear`: clears that place
- `video_sync_sensitivity_set`: how events are detected
- `video_sync_calculate`: searches for matching offsets
- `video_sync_candidate_apply`: applies a candidate

### History

- `history_undo`: undoes the last edit
- `history_redo`: redoes it
