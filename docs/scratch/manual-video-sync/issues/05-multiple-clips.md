# How should the sync doctor handle multiple clips sequentially?

Status: closed
Labels: wayfinder:grilling

## Question

When OVRLEY expands to multiple clips per project, how does the sync doctor let the user align each clip one at a time against the same activity timeline? Decide the entry point, the clip-selection/listing pattern, whether offsets accumulate or are independent, and how the user knows which clips are already synced and which still need alignment.

## Blocking

- [Which sync workflow should OVRLEY support?](03-chosen-workflow.md)
- [What is the schematic UI blueprint for the sync-doctor screen?](04-ui-blueprint.md)

## Comments

## Resolution

When OVRLEY supports multiple clips, the sync doctor remains a **per-clip screen** with the same landmark-correlation workflow.

- **Clip selection** happens in the **left toolbar drawer**, not inside the sync-doctor screen itself.
- The user selects one clip at a time; the sync screen loads that clip’s landmarks, candidates, and offset.
- **State is fully per-clip** — each clip has its own offset, landmarks, and candidate history.
- **Status icons** in the clip drawer indicate:
  - auto-synced (default metadata sync),
  - manually synced,
  - or any other relevant state the project needs.

No special accumulation or cross-clip workflow is needed; clips are aligned independently against the same activity timeline.
