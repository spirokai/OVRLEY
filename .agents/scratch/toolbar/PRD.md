Status: ready-for-agent

# Left Toolbar and Pinnable Drawer

## Problem Statement

OVRLEY currently exposes the widget catalog through a dedicated left-side drawer with a narrow vertical text tab attached to the drawer itself. The drawer always overlays part of the editor workspace, always behaves as temporary UI, and cannot remain available while a user repeatedly adds widgets or works in the overlay editor. Its entry point and content are also coupled, so it does not provide a scalable shell for additional left-side tools.

Users need a persistent left toolbar that can host multiple tools over time and control one shared drawer. They also need to choose whether that drawer temporarily overlays the editor workspace or becomes a durable part of the application layout that keeps its content available while shifting the scene canvas and timeline aside.

## Solution

Add a thin vertical toolbar below the app header and along the full left edge of the main content area. The toolbar initially contains one Widgets tool. Selecting that tool opens the shared drawer immediately to the toolbar's right and displays the existing widget catalog.

The shared drawer supports overlay and pinned modes. In overlay mode it extends over the full scene-canvas and timeline region and retains the current widget drawer's temporary dismissal behavior. In pinned mode it becomes part of the application layout, remains visible, and causes the scene canvas and timeline to reflow into the remaining width. A pin control in the drawer changes between these modes.

The pinned preference and active tool persist in OVRLEY's settings store. OVRLEY restores a pinned drawer before rendering the application shell so the workspace does not visibly jump after startup. An unpinned drawer always starts closed. The architecture supports future toolbar tools through one active-tool and one shared-drawer model without implementing those additional tools now.

## User Stories

1. As an OVRLEY user, I want a toolbar along the left edge of the editor, so that shell tools have a stable and discoverable home.
2. As an OVRLEY user, I want the toolbar to begin below the app header, so that it does not compete with window and application controls.
3. As an OVRLEY user, I want the toolbar to extend to the bottom of the application, so that it visually anchors the complete editor workspace.
4. As an OVRLEY user, I want the toolbar to sit to the left of both the scene canvas and timeline, so that both workspace regions share one consistent left boundary.
5. As an OVRLEY user, I want the toolbar to remain thin, so that it consumes little editing space.
6. As an OVRLEY user, I want a Widgets tool in the toolbar, so that I can open the widget catalog.
7. As an OVRLEY user, I want the old protruding Widgets text tab replaced by the toolbar tool, so that there is one canonical drawer entry point.
8. As an OVRLEY user, I want selecting Widgets while its unpinned drawer is closed to open it, so that I can browse available widgets.
9. As an OVRLEY user, I want selecting Widgets again while its unpinned drawer is open to close it, so that the toolbar tool behaves as a toggle.
10. As an OVRLEY user, I want the drawer to appear immediately beside the toolbar, so that its content is visibly associated with the selected tool.
11. As an OVRLEY user, I want the unpinned drawer to overlay the editor workspace, so that temporarily opening a tool does not permanently reduce canvas space.
12. As an OVRLEY user, I want the unpinned drawer to cover the scene-canvas and timeline region vertically, so that it follows the toolbar's full-height geometry.
13. As an OVRLEY user, I want the unpinned drawer to retain an invisible backdrop, so that the scene remains visible for comparison while background interaction is blocked.
14. As an OVRLEY user, I want clicking outside an unpinned drawer to close it, so that temporary tool content is easy to dismiss.
15. As an OVRLEY user, I want Escape to close an unpinned drawer, so that its current keyboard dismissal behavior remains available.
16. As an OVRLEY user, I want adding a widget from an unpinned drawer to close it, so that the current quick-add workflow remains unchanged.
17. As an OVRLEY user, I want the existing `Alt+W` shortcut to toggle the unpinned Widgets drawer, so that my current keyboard workflow remains intact.
18. As an OVRLEY user, I want the toolbar to remain available while an unpinned drawer is open, so that the active tool continues to control the drawer.
19. As an OVRLEY user, I want a pin control in the drawer's top-right corner, so that I can make the drawer part of the layout.
20. As an OVRLEY user, I want pinning an open drawer to keep its content visible, so that changing layout mode does not interrupt my task.
21. As an OVRLEY user, I want a pinned drawer to remain visible, so that I can repeatedly use its content without reopening it.
22. As an OVRLEY user, I want the pinned drawer to occupy layout width rather than overlay the workspace, so that it does not obscure the scene canvas or timeline.
23. As an OVRLEY user, I want the pinned drawer to shift both the scene canvas and timeline to the right, so that the workspace remains fully visible.
24. As an OVRLEY user, I want the existing right control panel to remain fixed in place and width, so that pinning does not change the widget editor or scene-settings layout.
25. As an OVRLEY user, I want the scene canvas to refit after pinning or unpinning, so that it uses the newly available workspace width.
26. As an OVRLEY user, I want the timeline viewport to reflow after pinning or unpinning, so that its controls and lanes use the newly available workspace width.
27. As an OVRLEY user, I want no application-level horizontal scrolling after pinning, so that docking remains a true layout change rather than creating an oversized page.
28. As an OVRLEY user, I want pinning to remain available at every supported window width, so that the application does not override my explicit preference.
29. As an OVRLEY user, I want adding a widget from a pinned drawer to leave it open, so that I can add several widgets efficiently.
30. As an OVRLEY user, I want outside clicks to leave a pinned drawer open, so that interacting with the scene does not dismiss permanent tool content.
31. As an OVRLEY user, I want Escape to leave a pinned drawer and its temporary inner UI unchanged, so that pinned mode does not react to drawer dismissal commands.
32. As an OVRLEY user, I want selecting the active toolbar tool while the drawer is pinned to leave the drawer visible, so that only the pin control changes its permanent layout state.
33. As an OVRLEY user, I want `Alt+W` in pinned mode to select the Widgets tool without hiding the drawer, so that shortcut and toolbar behavior remain consistent.
34. As an OVRLEY user, I want unpinning to keep the drawer open as an overlay, so that I do not lose context when changing layout mode.
35. As an OVRLEY user, I want pinning and unpinning to animate, so that the relationship between the drawer and reflowing workspace remains clear.
36. As an OVRLEY user, I want scene-canvas and timeline pointer interactions blocked during the reflow animation, so that I cannot begin an interaction against moving geometry.
37. As an OVRLEY user, I want my pinned setting restored after restarting OVRLEY, so that my preferred workspace layout is durable.
38. As an OVRLEY user, I want a previously pinned drawer restored visibly open, so that the restored layout truthfully represents its pinned state.
39. As an OVRLEY user, I want the active drawer tool restored with a pinned drawer, so that the permanent drawer returns to the content I selected.
40. As an OVRLEY user, I want an unpinned drawer to start closed after restarting OVRLEY, so that incidental temporary visibility does not become persistent state.
41. As an OVRLEY user, I want OVRLEY to resolve the drawer preference before showing the application shell, so that the workspace does not jump from unpinned to pinned after startup.
42. As an OVRLEY user, I want to see a full-shell `OVRLEY is starting...` state while the preference is resolved, so that startup progress is explicit.
43. As an OVRLEY user, I want a missing, malformed, or unreadable drawer preference to start unpinned, so that optional shell state cannot prevent OVRLEY from opening.
44. As an OVRLEY user, I want a failed preference write to leave my current session layout unchanged, so that a storage problem does not reverse my explicit action.
45. As an OVRLEY user, I do not want preference read or write failures surfaced, so that optional toolbar persistence fails silently.
46. As an OVRLEY user, I want the existing right-side Widgets tab to remain the editor for widgets already in the template, so that adding and editing keep their current distinct responsibilities.
47. As an OVRLEY user, I want the left Widgets drawer to remain the catalog for adding widgets, so that this layout change does not redesign widget editing.
48. As a future OVRLEY tool user, I want all left toolbar tools to share one drawer, so that multiple panels never overlap or compete for layout width.
49. As a future OVRLEY tool user, I want exactly one drawer tool active whenever the drawer is visible, so that drawer content has one canonical owner.
50. As a future OVRLEY tool user, I want selecting another toolbar tool to replace drawer content in place, so that switching tools does not open another panel.
51. As a future OVRLEY tool user, I want the drawer's pinned or unpinned mode preserved while changing tools, so that selecting content does not unexpectedly change layout.
52. As a future OVRLEY tool user, I want pinning to belong to the shared drawer rather than an individual tool, so that there is one predictable workspace preference.
53. As a future OVRLEY tool user, I want inactive tool content unmounted, so that hidden tools do not keep local UI state or effects alive indefinitely.
54. As an OVRLEY maintainer, I want toolbar selection, drawer visibility, and drawer pinning represented by one canonical layout model, so that future tools do not add independent drawer booleans.
55. As an OVRLEY maintainer, I want preference validation and normalization performed once when settings enter the application, so that presentation components consume canonical layout state.
56. As an OVRLEY maintainer, I want the toolbar and drawer shell separated from widget catalog content, so that future tools can reuse the shell without depending on widget behavior.
57. As an OVRLEY maintainer, I want the toolbar, drawer, workspace, and right control panel to remain in a single explicit horizontal layout, so that overlay and pinned geometry are understandable.

## Implementation Decisions

- The app shell owns the vertical toolbar, shared drawer shell, workspace allocation, startup gate, and pin/unpin transition boundary.
- The toolbar occupies a fixed `3rem` width before the application's existing global UI scale and spans the complete main-content height below the app header.
- The drawer occupies a fixed `15rem` width before global UI scale. Toolbar width is additional and is not included in drawer width.
- The canonical pinned horizontal order is toolbar, drawer, scene-canvas/timeline workspace, then the existing right control panel.
- In overlay mode, the toolbar remains in normal layout and the drawer is positioned immediately to its right over the scene-canvas and timeline workspace. The overlay does not cover the app header or toolbar.
- The overlay drawer has a transparent click-blocking backdrop over the workspace. The toolbar and drawer remain interactive above it.
- The current protruding text tab is removed. The Widgets tool in the toolbar becomes the only left-side entry point for the widget catalog.
- The toolbar and shared drawer are reusable app-shell presentation. Widget catalog content remains owned by the widget-drawer feature and is rendered as the active tool's content.
- The shared drawer always has one canonical active tool when visible. Widgets is the only tool in this release.
- Inactive future tool content is unmounted rather than visually hidden.
- Pinning is drawer-level state. It does not belong to the active tool and remains unchanged when the active tool changes.
- The layout store replaces the widget-specific open boolean and actions with canonical drawer state and behavior that represent visibility, active tool, pinning, initialization, tool selection, dismissal, and pin changes.
- A pinned drawer is necessarily visible. Broken state combinations such as pinned with no active tool or pinned but hidden must not be representable through normal store actions.
- Selecting a tool in unpinned mode opens it when closed, closes it when it is already active and open, and replaces the active content when another tool is selected.
- Selecting a tool in pinned mode makes it active but never hides the drawer.
- Unpinned mode retains all existing widget drawer dismissal behavior: outside click, Escape, and successful widget addition close the drawer.
- Pinned mode has no drawer Escape behavior. Escape must not close the drawer or temporary UI nested inside the drawer.
- Successful widget addition leaves a pinned drawer visible and closes an unpinned drawer.
- Unpinning keeps the drawer visible and converts it to overlay mode. It does not dismiss the drawer.
- `Alt+W` delegates to the same Widgets tool-selection behavior as the toolbar. It toggles an unpinned Widgets drawer and only selects Widgets in pinned mode.
- The right-side Widgets editor tab and right control panel retain their existing responsibilities and fixed width.
- Pinning allocates drawer width from the center workspace. The scene canvas and timeline reflow in the remaining width without application-level horizontal scrolling.
- The existing canvas and timeline resize mechanisms receive the changed container geometry naturally; no alternate geometry model or manual width alias is introduced.
- Pinning remains permitted at the application's minimum supported window size. No responsive auto-unpin, disabled-pin threshold, or right-panel collapse is introduced.
- Pin/unpin uses one coordinated layout transition for drawer allocation and workspace reflow. Workspace pointer input is blocked for the duration of this transition and restored when it completes.
- The pin control is located in the drawer's top-right corner. Final icon selection and non-structural visual styling follow existing OVRLEY and Lucide conventions and are not product-level decisions.
- The durable preference is stored in the existing `ovrley-settings.json` Tauri store. It records the drawer-level pinned boolean and active tool only; unpinned open/closed state is not persisted.
- Preference ingress accepts documented optional absence. Missing, malformed, or unreadable preference data normalizes to the canonical unpinned startup state without a user-facing or logged error.
- On startup, a valid pinned preference restores the recorded active tool and a visible pinned drawer. A valid unpinned preference restores the drawer closed.
- The main application shell is withheld until drawer preference hydration completes. A full-shell startup view displays exactly `OVRLEY is starting...` during hydration.
- Preference hydration and persistence are owned by a shell-level hook or utility boundary, not by presentational toolbar or drawer components.
- Pin changes update current in-memory layout state immediately and persist asynchronously. A failed write is silent and does not roll back the current session state.
- Persistence does not use Zustand persistence middleware, browser storage, compatibility aliases, or a second settings file.
- No backend or Rust changes are required; this is a React app-shell, Zustand layout, and existing Tauri preference-store feature.

## Testing Decisions

- No automated tests will be added for this feature, by explicit product decision.
- No feature-specific manual test plan is required by this spec.
- Implementation verification is limited to the repository's normal frontend lint and static formatting checks. These checks do not substitute for behavioral coverage.
- Existing widget drawer, layout slice, and shell keyboard tests may require compatibility updates because canonical layout APIs and public behavior change. Such maintenance must not introduce new feature assertions.
- The absence of behavioral tests is an accepted regression risk for startup hydration, persistence failure handling, keyboard behavior, dismissal differences between modes, and animated workspace reflow.

## Out of Scope

- Adding toolbar tools other than Widgets.
- Choosing final icons, typography, colors, shadows, border treatment, tooltips, or other non-structural visual polish beyond following existing OVRLEY conventions.
- Moving widget property editing from the right-side Widgets tab into the left drawer.
- Changing widget catalog categories, availability indicators, option popovers, or which widgets can be added.
- Changing widget creation, selection, template mutation, or undo/redo behavior.
- Making the drawer resizable or allowing the user to configure toolbar or drawer width.
- Persisting temporary unpinned drawer visibility.
- Persisting future tool-local UI state.
- Responsive auto-unpinning, pin restrictions at narrow widths, collapsing the right control panel, or adding application-level horizontal scrolling.
- Changing the existing global UI scaling strategy.
- Introducing browser storage, Zustand persistence middleware, a new native command, or a new persistence file.
- Adding new accessibility interaction requirements such as a focus trap or toolbar arrow-key navigation beyond preserving valid button semantics and labels.
- Automated feature tests or a feature-specific manual acceptance pass.

## Further Notes

- The toolbar and shared drawer must be specified and implemented as a separate feature module under `app/src/features/`, rather than being absorbed into `app-shell` or `widget-drawer`.
- In this document, scene canvas refers to the overlay editor's central visual editing surface, timeline refers to the Overlay Player region below it, widget catalog refers to the left-side quick-add content, and Widgets editor refers to the existing right control-panel tab.
- The stored drawer preference is optional shell presentation state, not user template config or external activity data. The agreed malformed-value fallback is therefore intentionally permissive even though required OVRLEY config remains a strict contract.
- The startup gate exists specifically to prevent a visible unpinned-to-pinned layout jump. It should not delay preference-independent background bootstrap work unless composition requires it.
- The `15rem` drawer and `3rem` toolbar values are logical CSS dimensions inside the existing globally scaled app shell.
- The implementation must preserve one canonical naming scheme for the shared drawer. Widget-specific names should remain only in the widget catalog content and Widgets tool identifier, not in generic layout state.
