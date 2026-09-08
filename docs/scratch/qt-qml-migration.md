# Qt QML Migration

Your task is to analyze `app/` and create a strategy/framework for migrating the existing ReactJS frontend into a Qt QML frontend, including the order of tasks.

## GOAL

Migrate the existing ReactJS frontend (`app/`) FULLY into a Qt QML frontend (`src-tauri/ovrley_qml/`) and Rust. No ReactJS remains at the end of the migration.

## NON-NEGOTIABLE REQUIREMENTS

- The existing ReactJS frontend must remain fully functional until the Qt QML frontend is fully implemented.
- Qt QML must be implemented as a parallel frontend, started with a separate dev command.
- Always migrate one feature at a time.
- Do not remove the React implementation of a feature until its QML replacement is functional.

## IMPORTANT INSTRUCTIONS

- Implement a Qt QML equivalent of hot module reload based on Gyroflow:
  https://github.com/gyroflow/gyroflow/blob/master/src/ui_live_reload.cpp

- Styling must be implemented natively in Qt QML using QML singletons/themes and reusable QML components.

- Existing Zustand state management must be migrated into an appropriate Qt/Rust architecture rather than mechanically recreated.
  - Application/domain/project state should generally live in Rust.
  - Temporary UI-only state can remain in QML.
  - Prefer typed Rust - QML controllers/models using `qmetaobject-rs`, properties, signals, slots/methods, and list models.
  - Inspect Gyroflow as the PRIME example of a Qt QML frontend with a Rust backend:
    https://github.com/gyroflow/gyroflow
  - Avoid copying Gyroflow's architecture blindly, especially a single oversized controller if OVRLEY can use smaller focused controllers/models.

- Components and shell in `app/src/features` and `app/src/components` should likely be migrated early, but determine the exact order from dependencies found during analysis.

- Widget preview MUST be migrated as THE VERY LAST thing.
  - It will reuse the existing Rust widget renderer through Rust - QML bindings.
  - Static layer caching must first be investigated/solved; I believe rendering currently effectively uses a single layer.
  - The rest of the QML frontend should not depend on the final widget-preview implementation being ready.

- Reuse `app/src/i18n/locales` for translations.
  - The existing JSON files remain the canonical translation source.
  - Do not create a second independently maintained translation source.
  - Determine the cleanest way to expose/use these translations from QML.

- Video playback should reuse Gyroflow's stack from:
  https://github.com/AdrianEddy/qml-video-rs

  Study Gyroflow's implementation:
  https://github.com/gyroflow/gyroflow/blob/master/src/ui/VideoArea.qml

  Reuse the playback/decoding architecture, style it for OVRLEY, and adapt it to OVRLEY's timeline requirements including video + activity synchronization.

- OVRLEY is open source and GPL licensed. Do not worry about licensing.

- Use Qt Quick/QML and Qt Quick Controls by default. Assess whether Qt Widgets are actually necessary for any UI, especially:
  - `features/components/app-shell/ControlPanel.jsx`
  - `features/toolbar/components/VerticalToolbar.jsx`
  - `features/scene-settings/components/SceneSettingsTab.jsx`

  Do not introduce Qt Widgets unless there is a concrete technical reason QML cannot reasonably handle the requirement.

- Much of `app/src/lib` is React-Rust glue, formatting logic already present in Rust, or widget drafting/resolving/presentation logic that may be superseded by direct Rust bindings. Do not port this code blindly.

- When migrating any utility function:
  - first check whether it already exists in Rust;
  - if yes, reuse it;
  - if it is close, adapt/refactor the Rust implementation;
  - otherwise prefer implementing it in Rust and exposing it to QML unless it is clearly presentation-only logic.

- Replace APIs in `src/api/backend.js` with appropriate Rust - QML APIs.
  - Inventory Tauri `invoke` calls, events/listeners, dialogs, filesystem calls, and other frontend/backend communication.
  - Prefer typed Rust controllers/models over recreating the JavaScript API layer.
  - Consult Gyroflow where useful.

- Analyze React hooks and determine whether their behavior belongs in QML or Rust. Business logic and reusable application logic should generally move into Rust rather than being recreated as QML equivalents of React hooks.

- Preserve existing Rust functionality wherever practical. This migration should not rewrite working Rust code unnecessarily.

- Ignore FIT/GPX/SRT/IGC parsing in `lib/activity`. We will rewrite those in Rust later or use appropriate Rust libraries.

## OUTPUT

Base the strategy on the ACTUAL OVRLEY codebase, not generic React-to-QML advice.

Identify:

- what should remain/move to Rust;
- what maps directly to QML;
- what React/TypeScript code becomes obsolete;
- required Rust - QML controllers/models/APIs;
- reusable QML component groups;
- state ownership;
- feature dependencies;
- coarse folder structure of the new QML frontend;
- architectural risks/blockers;
- and the recommended migration order while keeping the existing React frontend functional.
