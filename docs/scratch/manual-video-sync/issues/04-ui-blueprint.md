# What is the schematic UI blueprint for the sync-doctor screen?

Status: closed
Labels: wayfinder:grilling

## Question

What is the schematic UI blueprint (layout, panels, controls, and flow) for the sync-doctor screen? It must support the chosen workflow, show the right data signals, let the user mark and converge video and data points, and verify the result. Decide: side-by-side video + data, combined overlay, split timeline, or another arrangement. Specify the key components and their responsibilities at a schematic level (no production code or runnable prototype).

## Blocking

- [Which sync workflow should OVRLEY support?](03-chosen-workflow.md)

## Comments

## Resolution

The sync-doctor screen is a **persistent toolbar screen** in OVRLEY. The user can switch between it and the rest of the editor/preview at any time.

### Layout

- **Toolbar (top):**
  - **Mark stop location**
  - **Mark turn location**
  - **Mark map point** (active once real map tiles are available)
- **Main stage (center):**
  - Large **video player**.
  - Lightweight **overlay widgets** directly on the video (reusing existing components): current speed and a mini route map. These update live as the user selects candidate offsets, so verification is immediate.
- **Context panel (below or beside video, toggleable):**
  - **Map view** by default once map tiles exist.
  - **Data graphs** (speed + heading) as supplementary feedback, showing detected events. Not hidden, but secondary to the map/video preview.
- **Right-hand side panel:**
  - **Landmarks list** — marked video landmarks with type icons and frame thumbnails.
  - **Subtle hint** when candidate conditions are not yet met (e.g., “Add one more turn/stop, or mark a map point”).
  - **Candidate offsets toggle group** — computed offsets appear here as the conditions are fulfilled. Selecting a candidate live-applies that offset to the video preview and overlay widgets.
  - **Existing numeric offset input** — final fine-tuning, reusing the current component.
  - **Reset to auto-sync** — discards manual offset and falls back to automatic metadata sync.
  - **Clear landmarks** — removes all landmarks and candidates to start over.

### Key interactions

1. User selects a landmark tool and marks a frame in the video (and, for map points, the corresponding location on the map).
2. The UI computes candidate offsets:
   - A single **map landmark** yields one candidate.
   - **Two or more turn/stop landmarks** yield candidates via correlation.
   - Mixed landmark sets are combined when they constrain the offset.
3. Candidates quietly appear in the toggle group as soon as conditions are met.
4. Selecting a candidate updates the overlay widgets and video preview in real time.
5. The user fine-tunes with the numeric offset input if needed.

### Out of scope for this blueprint

- Production code or runnable prototype.
- Exact visual styling, animation, or responsive breakpoints.
