# Fugue UI Design Document

This document outlines the architecture for the Fugue visualization and composition UI.

## Goals

1. **Visualize active fugues** - Show notes in a step sequencer grid, CC data as line graphs
2. **Show playback position** - Real-time playhead synced to DAW transport
3. **Compose fugues** - Interactive UI for creating/editing fugues (for testing)
4. **Minimize duplication** - Share components between viewer and composer modes

## Architecture Overview

### Shared Core: `FugueGrid` Component

The key insight is that **viewing and composing use the same visual representation**. We create a single `FugueGrid` component that handles both modes:

```
┌─────────────────────────────────────────────────────────────────┐
│ FugueGrid (shared component)                                    │
│ ┌─────────────────────────────────────────────────────────────┐ │
│ │ Props:                                                      │ │
│ │   - events: TimedFugueEvent[]                               │ │
│ │   - durationBeats: number                                   │ │
│ │   - playheadBeat?: number (undefined = no playhead)         │ │
│ │   - mode: 'view' | 'edit'                                   │ │
│ │   - onEventsChange?: (events) => void                       │ │
│ │   - noteRange?: { min: number, max: number }                │ │
│ │   - ccFilters?: number[] (which CCs to show)                │ │
│ └─────────────────────────────────────────────────────────────┘ │
│                                                                 │
│ Internal structure:                                             │
│ ┌─────────────────────────────────────────────────────────────┐ │
│ │ NoteGrid (step sequencer style)                             │ │
│ │ - Y axis: note pitches (piano roll style)                   │ │
│ │ - X axis: beats                                             │ │
│ │ - Cells: note on/off with velocity (color intensity)        │ │
│ │ - Edit mode: click to toggle, drag to draw                  │ │
│ └─────────────────────────────────────────────────────────────┘ │
│ ┌─────────────────────────────────────────────────────────────┐ │
│ │ CCGraph (automation lanes)                                  │ │
│ │ - One lane per CC number                                    │ │
│ │ - Line graph showing value over time                        │ │
│ │ - Edit mode: click to add points, drag to adjust            │ │
│ └─────────────────────────────────────────────────────────────┘ │
│ ┌─────────────────────────────────────────────────────────────┐ │
│ │ Playhead (vertical line overlay)                            │ │
│ │ - Position synced to transport                              │ │
│ │ - Shows current beat within fugue                           │ │
│ └─────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

### Component Hierarchy

```
App
├── FuguePanel (new section)
│   ├── FugueList
│   │   └── FugueListItem (shows tag, progress, controls)
│   │
│   ├── FugueViewer (wraps FugueGrid in view mode)
│   │   ├── FugueGrid mode="view"
│   │   └── Transport info overlay
│   │
│   └── FugueComposer (wraps FugueGrid in edit mode)
│       ├── FugueGrid mode="edit"
│       ├── Duration/loop controls
│       ├── Quantize/cancel mode selectors
│       └── Queue button
```

### Data Flow

```
┌──────────────────────────────────────────────────────────────────┐
│                         Backend (Rust)                            │
├──────────────────────────────────────────────────────────────────┤
│                                                                   │
│  FugueBridge                                                      │
│  ├── get_fugue_info() → basic info (id, tag, progress)           │
│  ├── get_fugue_details(id) → full definition with events  [NEW]  │
│  ├── get_transport_state() → current beat, tempo, playing [NEW]  │
│  └── queue/cancel methods                                         │
│                                                                   │
│  GUI Routes (droplets://api/...)                                  │
│  ├── /fugues → list active fugues with basic info                │
│  ├── /fugue/{id} → full fugue definition for visualization       │
│  ├── /transport → current transport state                        │
│  └── /queue_fugue (POST) → queue a new fugue from UI             │
│                                                                   │
└──────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────────┐
│                       Frontend (React)                            │
├──────────────────────────────────────────────────────────────────┤
│                                                                   │
│  useFugueData() hook                                              │
│  ├── Polls /fugues at ~100ms for list + progress                 │
│  ├── Fetches /fugue/{id} when selection changes                  │
│  └── Polls /transport at ~50ms for smooth playhead               │
│                                                                   │
│  State:                                                           │
│  ├── fugueList: FugueInfo[]                                      │
│  ├── selectedFugue: FugueDefinition | null                       │
│  ├── transport: { beat, tempo, playing }                         │
│  └── composerDraft: FugueDefinition (for editing)                │
│                                                                   │
└──────────────────────────────────────────────────────────────────┘
```

## Detailed Component Designs

### 1. FugueGrid (Shared Core)

The heart of the UI. Renders both notes and CC automation.

**Props:**
```typescript
interface FugueGridProps {
  events: TimedFugueEvent[];
  durationBeats: number;

  // Playback
  playheadBeat?: number;        // undefined = no playhead shown
  isPlaying?: boolean;          // affects playhead animation

  // Display options
  mode: 'view' | 'edit';
  noteRange?: { min: number; max: number };  // default: auto from events
  visibleCCs?: number[];        // which CC lanes to show
  beatsPerBar?: number;         // for grid lines (default: 4)

  // Edit mode callbacks
  onEventsChange?: (events: TimedFugueEvent[]) => void;

  // Sizing
  pixelsPerBeat?: number;       // horizontal zoom
  noteHeight?: number;          // vertical note size
}
```

**Internal Components:**
- `NoteGrid` - Piano roll style grid for note events
- `CCLane` - Single CC automation lane with line graph
- `Playhead` - Animated vertical line
- `GridLines` - Beat/bar markers

**Edit Interactions:**
- Click on empty cell → add note (default velocity 100)
- Click on existing note → remove it
- Drag horizontally → adjust note length (future)
- Shift+click → adjust velocity
- CC lane: click to add point, drag to move

### 2. FugueViewer

Wrapper for viewing active fugues with transport sync.

```typescript
interface FugueViewerProps {
  fugue: FugueDefinition;
  transport: TransportState;
}
```

Responsibilities:
- Calculate playhead position from transport + fugue start beat
- Handle loop wrap-around for playhead
- Show "waiting for quantize" state
- Read-only display (mode="view")

### 3. FugueComposer

Wrapper for creating/editing fugues.

```typescript
interface FugueComposerProps {
  initialFugue?: FugueDefinition;  // for editing existing
  onQueue: (fugue: FugueDefinition) => void;
  onCancel: () => void;
}
```

Responsibilities:
- Manage draft state
- Duration/beats input
- Loop mode selector (once/times/forever)
- Quantize mode selector (immediate/beat/bar)
- Cancel mode selector (none/tag/all)
- Tag input
- "Queue" button to send to audio thread

### 4. FugueList

Shows all active fugues with selection and controls.

```typescript
interface FugueListProps {
  fugues: FugueInfo[];
  selectedId?: number;
  onSelect: (id: number) => void;
  onCancel: (id: number) => void;
}
```

Each list item shows:
- Tag (or "Untitled")
- Progress bar (current beat / duration)
- Loop indicator (1/3, ∞, etc.)
- Cancel button

## Backend Changes

### New API Routes

Add to `src/gui/routes.rs`:

```rust
// GET /fugues - list all active fugues
fn get_fugues() -> String {
    // Returns FugueInfo[] from FugueBridge::get_fugue_info("default")
}

// GET /fugue/{id} - get full fugue definition
fn get_fugue_details(id: u64) -> String {
    // Returns FugueDefinition with all events
    // Needs new FugueBridge method to fetch by ID
}

// GET /transport - current transport state
fn get_transport() -> String {
    // Returns { beat: f64, tempo: f64, playing: bool }
    // Needs transport state cache (updated by audio thread)
}

// POST /queue_fugue - queue a fugue from UI
fn queue_fugue(body: QueueFugueRequest) -> String {
    // Parse and queue via FugueBridge
}
```

### Extended FugueInfo

For the viewer, we need the full event list. Options:

**Option A: Separate endpoint**
- `/fugues` returns basic `FugueInfo[]`
- `/fugue/{id}` returns full `FugueDefinition`
- Pro: Efficient for list view
- Con: Extra request for details

**Option B: Extended info struct**
- Include events in `FugueInfo`
- Pro: Single request
- Con: Larger payload for list

**Recommendation: Option A** - keeps list polling lightweight.

### Transport State Cache

Need to expose transport info to the GUI. Add to `FugueBridge`:

```rust
// Shared transport state (updated by audio thread, read by GUI)
struct TransportCache {
    beat: AtomicF64,  // or use ArcSwap
    tempo: AtomicF64,
    playing: AtomicBool,
}
```

## CSS Architecture

Use CSS modules or a consistent naming convention:

```css
.fugue-grid { }
.fugue-grid__note-area { }
.fugue-grid__cc-lanes { }
.fugue-grid__playhead { }
.fugue-grid__cell { }
.fugue-grid__cell--active { }
.fugue-grid__cell--editing { }
```

## Implementation Order

1. **Backend routes** (`/fugues`, `/fugue/{id}`, `/transport`)
2. **Transport cache** in FugueBridge
3. **FugueGrid component** (view mode first)
4. **FugueList component**
5. **FugueViewer** (integrate grid + transport)
6. **FugueGrid edit mode** (click handlers)
7. **FugueComposer** (controls + draft state)
8. **Integration** into main App

## Drag-out / drag-in (Features 15 + 16)

The sequencer supports both directions of the round-trip between
Droplets and the DAW:

- **Drag-out.** Press and drag on any fugue row (or the viewer header)
  to start a native OS drag carrying a `.mid` file. Drop onto a DAW
  arranger track, the Finder/Explorer, or any app that accepts file
  drops. The `⇣ Drag all <N>` header button bundles every active fugue
  on the instance into one multi-track SMF; tag groups with different
  loop lengths are LCM-stretched so the clip loops cleanly in the DAW.
- **Drag-in.** Drop a `.mid` (or `.midi`) anywhere on the sequencer
  panel. The panel highlights with a dashed outline while the drag is
  over it. Each track in the file becomes one fugue queued on the
  currently-selected instance. Tags come from the SMF `TrackName`
  meta; tracks without names get synthetic `imported-N` labels.

### Platform caveats

- **macOS Gatekeeper first-drag.** The drag-out writes its temp
  `.mid` under `$TMPDIR/droplets-<ts>.mid`. On a fresh macOS install
  the first time you drop that file onto a DAW track, Gatekeeper may
  ask you to confirm opening a file "from an unidentified developer."
  This is a one-time prompt — subsequent drags of files from the
  same session go through without dialog. Dropping onto Bitwig
  always works for us; Logic and Live pre-11 occasionally bounce the
  first drop silently if Gatekeeper quarantined the temp file.
- **Linux.** `drag` crate doesn't support GTK-less windows (what CLAP
  plugins live inside), so the plugin falls back to revealing the
  `.mid` in the user's file manager. The user then drags from there.
- **Bitwig launcher clips.** Bitwig refuses to export clip-launcher
  clips as `.mid` — its only clip-launcher export is "Save Launcher
  Clip to Library" which writes a proprietary `.bwclip`. To round-
  trip a launcher clip today, drag it into the Bitwig arranger
  timeline first; arranger clips drag out as `.mid` normally.
  Native `.bwclip` support is tracked as Feature 23 post-demo.

## Future Enhancements

- Zoom controls for grid
- Piano keyboard sidebar for note reference
- Velocity editing mode
- Copy/paste fugues
- Save/load fugue presets
- Multi-fugue view (stacked)
