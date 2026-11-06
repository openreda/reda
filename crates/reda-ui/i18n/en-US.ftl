# REDA's English UI strings. Keys are used through `tr!("key")` in crates/reda-ui/src; a
# test fails when a used key is missing here.

## Start page

start-page-title = Start
start-page-heading = Welcome to REDA
start-new-project = New Project…
start-open-project = Open Project…
start-recent-projects = Recent Projects
start-no-recent-projects = No recent Projects yet.
start-project-missing = This Project is no longer there.
start-remove-recent = Remove { $name } from recent Projects

## Window

window-title = REDA
window-title-project = { $project } — REDA
document-area-empty = No Document open.
status-ready = Ready
status-dismiss = Dismiss
status-panels = Panels
status-grid = Grid: { $grid }
status-active = Active: { $kind }
status-cursor = X { $x }  Y { $y }
document-kind-schematic = Schematic Sheet
document-kind-pcb = PCB Document
document-kind-symbol = Symbol
document-kind-footprint = Footprint
document-kind-part = Part
active-bar-more = More Tools
panels-reopen = { $panel } (closed)
floating-window-title = REDA
floating-window-title-tab = { $tab } – REDA
status-tool-cancelled = { $tool } was cancelled: an { $source } edit changed what it was using.
tool-wire = Wire

## The bridge: agents connecting through the control port

bridge-indicator-off = Agents: off
bridge-indicator-on = Agents: on
bridge-indicator-connected = Agents: { $count } connected
bridge-switch = Let agents connect
bridge-connected = Connected now
bridge-allowed = Allowed agents
bridge-none-allowed = None yet
bridge-forget = Forget
bridge-settings = Agent Settings…
bridge-error = The bridge could not start: { $error }
bridge-ask-title = Allow this agent?
bridge-ask-body = { $agent } ({ $source }) wants to read and edit the open Project. It can't save: its changes show here as unsaved, and you can undo them.
bridge-allow = Allow
bridge-deny = Deny

## Docking

panel-projects = Projects
panel-properties = Properties
panel-messages = Messages
panel-library = Library
panel-layers = Layers
panel-journal = Journal
panel-empty = Nothing to show yet.
dock-expand = Expand Dock
tab-close = Close { $tab }
tab-menu-move-to = Move to
tab-menu-left-dock = Left Dock
tab-menu-right-dock = Right Dock
tab-menu-bottom-dock = Bottom Dock
tab-menu-new-window = New Window
tab-menu-split-right = Split Right
tab-menu-split-down = Split Down
tab-menu-close = Close
tab-menu-collapse-dock = Collapse Dock
tab-menu-not-in-dock = This tab is not in a Dock.

## Drag and drop: the drop-target buttons shown while a tab is dragged

drop-into-group = Drop into Group
drop-split-left = Drop to the Left
drop-split-right = Drop to the Right
drop-split-up = Drop Above
drop-split-down = Drop Below
drop-dock-left = Drop in Left Dock
drop-dock-right = Drop in Right Dock
drop-dock-bottom = Drop in Bottom Dock
drop-refused = Can't drop here: { $reason }

## Journal panel

journal-export = Export…
journal-export-hint = Save this session's Commands as JSON lines that `reda run --script` replays. Layout and Panel Commands are left out.
journal-export-title = Export the Journal
journal-export-filter = Journal script
journal-count = { $count ->
    [one] 1 Command
   *[other] { $count } Commands
}
journal-empty = No Commands run yet.
journal-column-number = #
journal-column-command = Command
journal-column-source = Source
journal-column-result = Result
journal-ok = OK
journal-error = { $code }: { $message }
journal-source-gui = App
journal-source-cli = CLI
journal-source-mcp = MCP
journal-export-failed = Could not export the Journal: { $error }

## Projects panel

projects-none = No Project is open.
projects-dirty = { $name } *
projects-settings = Project Settings…
projects-save-all = Save All
projects-show-in-folder = Show in Folder
projects-not-yet = Not available yet.
projects-schematic = Schematic
projects-add-sheet = Add Sheet
projects-open = Open
projects-rename = Rename
projects-rename-field = New name for { $name }
projects-move-up = Move Up
projects-move-down = Move Down
projects-cannot-move = It cannot move further.
projects-delete = Delete
projects-delete-sheet = Delete Sheet
projects-confirm-delete = Delete { $name }? It holds { $count } { $count ->
    [one] Component, which is
   *[other] Components, which are
} deleted with it.
projects-pcb = PCB
projects-no-pcb = No PCB Document.
projects-add-pcb = Add PCB Document
projects-pcb-exists = The Project already has a PCB Document.
projects-board-setup = Board Setup…
projects-output-jobs = Output Jobs
projects-no-jobs = No Output Jobs.
projects-new-job = New Job
projects-no-fab-preset = The Project has no Fab Preset for a job to use.
projects-run-job = Run
projects-edit-job = Edit…
projects-duplicate = Duplicate
projects-libraries = Libraries
projects-no-libraries = No Library folders.
projects-add-folder = Add Folder…
projects-remove = Remove

## Menus, toolbars and the Active Bar

# Menu and submenu titles, named by the bar data files in crates/reda-ui/bars.
menu-file = File
menu-edit = Edit
menu-view = View
menu-view-panels = Panels
menu-view-docks = Docks
menu-view-theme = Theme
menu-design = Design
menu-place = Place
pcb-user-origin-reset = Reset User Origin
menu-library = Library
menu-help = Help
bars-no-active-document = no Document is active
bars-nothing-selected = nothing is selected
bars-clipboard-empty = nothing has been copied
menu-edit-cut = Cut
menu-edit-copy = Copy
menu-edit-paste = Paste
menu-edit-duplicate = Duplicate
menu-edit-delete = Delete
menu-edit-rotate = Rotate
menu-edit-mirror-x = Mirror Left to Right
menu-edit-mirror-y = Mirror Top to Bottom
menu-edit-align = Align
menu-edit-align-left = Align Left
menu-edit-align-right = Align Right
menu-edit-align-top = Align Top
menu-edit-align-bottom = Align Bottom
menu-edit-align-center-x = Align Centres on a Vertical Line
menu-edit-align-center-y = Align Centres on a Horizontal Line
menu-edit-distribute-horizontal = Distribute Horizontally
menu-edit-distribute-vertical = Distribute Vertically
menu-edit-snap-to-grid = Snap to Grid
menu-view-grid = Snap Grid
menu-view-clear-highlight = Clear Net Highlight
menu-view-grid-10mil = 10 mil
menu-view-grid-25mil = 25 mil
menu-view-grid-50mil = 50 mil
menu-view-grid-100mil = 100 mil
menu-view-grid-0-25mm = 0.25 mm
menu-view-grid-0-5mm = 0.5 mm
menu-view-grid-1mm = 1 mm
menu-view-grid-2-5mm = 2.5 mm
bars-unknown-command = there is no Command { $command }

# Command and Preset titles the UI shows instead of the engine's: command-<id>-title, with
# the id's dots and colon written as dashes.
command-view-theme-set-follow_os-title = Follow the System
command-view-theme-set-dark-title = Dark
command-view-theme-set-light-title = Light

## Dialogs

dialog-cancel = Cancel
dialog-open-project-title = Open Project
dialog-project-filter = REDA Project
dialog-location-title = Choose Where to Create the Project
dialog-library-folder-title = Choose a Library Folder
new-project-title = New Project
new-project-default-name = Untitled
new-project-name = Name
new-project-location = Location
new-project-browse = Browse…
new-project-create = Create
new-project-fab-preset = Fab Preset
new-project-fab-preset-note = The board rules and the Output Job can be changed later.
new-project-board = Board
new-project-draw-later = Draw later
new-project-rectangle = Rectangle
new-project-width = Width (mm)
new-project-height = Height (mm)
new-project-corner-radius = Corner radius (mm)

## Help: the About box and the update check

about-title = About REDA
about-tab-about = About
about-tab-licences = Licences
about-version = Version { $version }
about-summary = Electronic design automation: capture schematics, lay out PCBs and produce fab outputs.
about-licence = REDA is free software under the MIT licence. It ships with third-party data and Rust crates under their own licences, listed under Licences.
about-notices = Third-party notices
help-close = Close
update-title = Check for Updates
update-available = REDA { $latest } is available. This is { $current }.
update-current = REDA { $latest } is the newest release.
update-current-dev = This build ({ $current }) is newer than the newest release, { $latest }.
update-open-release = Open the release page
update-failed = REDA could not check for updates: { $why }

## Properties panel

properties-nothing-selected = Nothing selected
properties-selected = { $count } selected
properties-designator = Designator
properties-value = Value
properties-description = Description
properties-datasheet = Datasheet
properties-manufacturer = Manufacturer
properties-mpn = MPN
properties-keywords = Keywords
properties-show-field = Show { $field } on the sheet
properties-show-field-hint = Show on the sheet
properties-footprint = Footprint
properties-no-footprint = None
properties-footprint-different-parts = The selected Components are different Parts, with different Footprints to choose from
properties-not-fitted = Not Fitted
properties-net-class = Net Class
properties-default-net-class = Default
properties-new-field-name = New field
properties-new-field-value = Value of the new field
properties-add-field = Add Field
properties-placing = Placing { $part }: edits apply to this and later placements.

# The Library panel.
library-search = Search Parts
library-filter-library = Library
library-filter-type = Component Type
library-all-libraries = All Libraries
library-all-types = All Types
library-type-standard = Standard
library-type-mechanical = Mechanical
library-sheet-parts = Parts with a Symbol, to place on the sheet
library-board-parts = Mechanical Parts, to place on the board
library-no-document = Open a sheet or the PCB to place Parts.
library-no-parts = No Parts match.
library-no-symbol = No Symbol
library-in = In { $library }
library-unit = Unit { $unit }
library-footprint = Alternate Footprint
library-symbol-preview = Symbol preview
library-footprint-preview = Footprint preview
library-edit = Edit Part
library-new = New…
library-new-part = New Part…
library-new-symbol = New Symbol…
library-new-footprint = New Footprint…
library-import-part = Import Part…
library-new-library = New Library…
new-library-title = New Library
new-library-name = Name
new-library-for = For
new-library-for-project = This Project
new-library-for-user = Me (all Projects)
new-library-folder = Folder
new-library-folder-project-hint = in the Project folder
new-library-description = Description
new-library-create = Create
library-generate = Generate Footprint
library-refresh = Refresh

## Canvas

tab-switcher-title = Switch Tab
canvas-name-schematic = Schematic canvas: { $document }
canvas-name-pcb = PCB canvas: { $document }
canvas-name-symbol = Symbol canvas: { $document }
canvas-name-footprint = Footprint canvas: { $document }
canvas-nothing-selected = Nothing selected
canvas-selected = { $count ->
    [one] 1 object selected
   *[other] { $count } objects selected
}

## Layers panel

layers-no-pcb = The Project has no PCB Document.
layers-inactive = Inactive Layers
layers-inactive-normal = Normal
layers-inactive-dimmed = Dimmed
layers-inactive-hidden = Hidden
layers-from-bottom = View from bottom
layers-show = Show { $layer }
layers-colour = Colour of { $layer }
layers-objects = Objects
layers-type-footprints = Footprints
layers-type-tracks = Tracks
layers-type-vias = Vias
layers-type-zones = Zones
layers-type-keepouts = Keepouts
layers-type-free-pads = Free Pads
layers-type-text = Text
layers-type-graphics = Graphics
layers-ratsnest = Ratsnest
layers-ratsnest-nets = Ratsnest per Net
layers-units = Units
layers-grid = Snap grid
layer-top = Top { $kind }
layer-bottom = Bottom { $kind }
layer-copper = Copper
layer-silkscreen = Silkscreen
layer-solder-mask = Solder Mask
layer-paste = Paste
layer-courtyard = Courtyard
layer-assembly = Assembly
layer-board-outline = Board Outline
layer-mechanical = Mechanical
layer-documentation = Documentation

## PCB canvas

pcb-cursor = X { $x }  Y { $y }  ·  grid { $grid }
pcb-grid = Grid { $grid }
pcb-no-outline = No Board Outline: draw one, or
pcb-create-rect = Create rectangle…
pcb-outline-title = Rectangular Board Outline
pcb-outline-width = Width
pcb-outline-height = Height
pcb-outline-create = Create
pcb-outline-cancel = Cancel
pcb-rotate = Rotate
pcb-flip = Flip Side
pcb-lock = Lock
pcb-unlock = Unlock
pcb-outline-from-selection = Board Outline from Selection
pcb-draw-needs-copper = Zones go on a copper Layer: make one active first.
pcb-draw-needs-graphics-layer = Text and graphics go on a copper, silkscreen, mechanical or documentation Layer: make one active first.
pcb-draw-arc-straight = The three points lie on one line: put the third off it to bend the arc.
pcb-text-title = Text
pcb-text-words = Text
pcb-text-place = Place Text
pcb-text-cancel = Cancel
pcb-measure = dX { $dx }  dY { $dy }  ·  { $distance }
status-zones-stale = Zones need refilling
status-zones-refill = Refill

## Errors

error-command = { $error }
error-show-in-folder = Could not open the file manager: { $error }
error-state-file = Could not update the recent Projects list: { $error }
status-last-project-missing = The last Project, { $project }, was not reopened: it is missing at { $path }
error-recovery = Recovery Snapshots: { $error }
error-project-held = { $project } is already open in another REDA window (process { $pid }); it stays there.

## Violations: one line per check id, built from its arguments. Lengths are in mm.

violation-unconnected-input = Input Pin { $pin } is not connected
violation-unconnected-pin = Pin { $pin } is not connected
violation-pin-conflict = Pin conflict on { $net }: { $pin_a } ({ $type_a }) with { $pin_b } ({ $type_b })
violation-pin-conflict-mixed = Pin types mix on { $net }: { $pin_a } ({ $type_a }) with { $pin_b } ({ $type_b })
violation-undriven-power = Power Net { $net } has no driver
violation-single-pin-net = Net { $net } connects only Pin { $pin }
violation-dangling-wire = Wire end at ({ $x }, { $y }) touches nothing
violation-unattached-label = { $name } is attached to nothing
violation-no-connect-connected = No-Connect Marker on connected Pin { $pin }
violation-multiple-net-names = Net { $net } has several names: { $names }
violation-single-sheet-port = Port { $name } is on only one sheet
violation-duplicate-designator = Designator { $designator } is used more than once
violation-unplaced-units = { $designator } has unplaced Units: { $units }
violation-unit-data-mismatch = The Units of { $designator } differ in: { $differs }
violation-clearance = Clearance { $actual } is below { $required }
violation-short-circuit = Short circuit between { $net_a } and { $net_b }
violation-track-width = Track width { $actual } is below { $minimum }
violation-hole-size = Hole size { $actual } is below { $minimum }
violation-annular-ring = Annular ring { $actual } is below { $minimum }
violation-hole-to-hole = Holes { $actual } apart, below { $required }
violation-edge-clearance = Copper { $actual } from the board edge, below { $required }
violation-keepout = Object inside a Keepout
violation-footprint-outside-board = Footprint outside the Board Outline
violation-unrouted = Unrouted connection on { $net }
violation-copper-without-net = Copper with no Net
violation-dangling = Unconnected end at ({ $x }, { $y })
violation-courtyard-overlap = Courtyards overlap
violation-silkscreen-over-pads = Silkscreen over exposed Pads
violation-out-of-sync = The PCB is out of sync with the schematic

## Messages panel

messages-general = General
message-preferences = Preferences: { $warning }
message-missing-font = Font “{ $font }” is not installed; its text is drawn in Noto Sans

## The Keymap's load errors, shown in Messages

keymap-unreadable = Keymap { $file } could not be read: { $error }
keymap-unknown-section = Keymap { $file }: “{ $section }” is not a section; use [[bind]] or [[unbind]]
keymap-bad-entry = Keymap { $file }: { $error }
keymap-unknown-field = a binding has an unknown field “{ $field }”
keymap-missing-field = a binding has no “{ $field }”
keymap-missing-field-for = the binding of { $key } has no “{ $field }”
keymap-not-a-string = “{ $field }” must be a string
keymap-bad-key = “{ $key }” is not a key: { $error }
keymap-bad-context = the binding of { $key } names “{ $context }”, which is not a context
keymap-preset-params = the binding of { $key } names a Preset, which has fixed params; give none
keymap-bad-number = a number in the params is not finite
keymap-clash = Keymap { $file }: { $key } is bound twice in the { $context } context; the first binding is kept
keymap-base-unbind = Keymap { $file }: only keymap.toml can unbind keys ({ $key })
keymap-unknown-command = Keymap { $file }: { $key } names { $command }, which is not a Command or Preset
messages-run-erc = Run ERC
messages-run-drc = Run DRC
messages-show-waived = Show waived
messages-erc-stale = ERC results are out of date: the design changed after the run.
messages-drc-stale = DRC results are out of date: the board changed after the run.
messages-none = No messages.
messages-group = { $document } ({ $count })
messages-waived-line = { $text } (waived)
messages-waive = Waive
messages-unwaive = Unwaive
messages-set-severity = Set severity of “{ $check }”
severity-error = Error
severity-warning = Warning
severity-off = Off

## Check titles, for "Set severity of …"

check-unconnected-input = Unconnected Input
check-unconnected-pin = Unconnected Pin
check-pin-conflict = Pin Conflict
check-pin-conflict-mixed = Mixed Pin Types
check-undriven-power = Undriven Power
check-single-pin-net = Single-Pin Net
check-dangling-wire = Dangling Wire
check-unattached-label = Unattached Label
check-no-connect-connected = No-Connect on a Connected Pin
check-multiple-net-names = Several Net Names
check-single-sheet-port = Port on One Sheet
check-duplicate-designator = Duplicate Designator
check-unplaced-units = Unplaced Units
check-unit-data-mismatch = Unit Data Mismatch
check-clearance = Clearance
check-short-circuit = Short Circuit
check-track-width = Track Width
check-hole-size = Hole Size
check-annular-ring = Annular Ring
check-hole-to-hole = Hole to Hole
check-edge-clearance = Edge Clearance
check-keepout = Keepout
check-footprint-outside-board = Footprint Outside Board
check-unrouted = Unrouted
check-copper-without-net = Copper Without Net
check-dangling = Dangling Copper
check-courtyard-overlap = Courtyard Overlap
check-silkscreen-over-pads = Silkscreen over Pads
check-out-of-sync = Out of Sync

## Crash recovery and Crash Reports

recovery-title = Recover Unsaved Work
recovery-intro = REDA did not close cleanly last time. It kept copies of these files' unsaved changes:
recovery-taken = Kept { $time }
recovery-disk-changed = The file changed on disk since. Restoring and saving would overwrite the newer content.
recovery-missing = The file is gone; restoring recreates it as unsaved.
recovery-unreadable = This copy cannot be read: { $error }
recovery-restore = Restore
recovery-discard = Discard
recovery-show-in-folder = Show in Folder
recovery-restore-all = Restore All
recovery-discard-all = Discard All
crash-title = REDA Closed Unexpectedly
crash-intro = REDA saved a Crash Report on this computer. Nothing has been sent anywhere.
crash-open-folder = Open Folder
crash-report-github = Report on GitHub…
crash-report-github-hint = Opens a pre-filled issue in your browser for you to review before sending.
crash-dismiss = Close

## Settings dialogs

settings-ok = OK
settings-apply = Apply
settings-invalid = Fix the flagged values first.
settings-page-invalid = { $page } (has invalid values)
project-settings-title = Project Settings
settings-page-properties = Properties
settings-page-erc = ERC
settings-page-fab-presets = Fab Presets
settings-page-libraries = Libraries
settings-title = Title
settings-revision = Revision
settings-date = Date
settings-author = Author
settings-company = Company
settings-board-thickness = Board thickness (mm)
settings-copper-weight = Copper weight
settings-fab-notes = The board thickness and copper weight are used only in fab notes. Give the copper weight as text, such as 1 oz.
settings-thickness-invalid = Not a length. Give mm, such as 1.6, or a unit, such as 63mil.
settings-thickness-positive = The board thickness must be more than zero.
settings-erc-choice = { $check }: { $severity }
settings-erc-matrix = Pin-type matrix (fixed): E is a Pin Conflict, W is Mixed Pin Types.
pin-type-input = Input
pin-type-input-short = In
pin-type-output = Output
pin-type-output-short = Out
pin-type-bidirectional = Bidirectional
pin-type-bidirectional-short = Bi
pin-type-tri-state = Tri-state
pin-type-tri-state-short = Tri
pin-type-passive = Passive
pin-type-passive-short = Pas
pin-type-power-in = Power In
pin-type-power-in-short = PI
pin-type-power-out = Power Out
pin-type-power-out-short = PO
pin-type-open-collector = Open Collector
pin-type-open-collector-short = OC
pin-type-open-emitter = Open Emitter
pin-type-open-emitter-short = OE
pin-type-not-connected = Not Connected
pin-type-not-connected-short = NC
settings-preset-none = The Project has no Fab Presets.
settings-preset-update = Update from Source…
settings-preset-will-update = Updates from its source on Apply.
settings-preset-keep = Keep Current
settings-preset-save-user = Save as User Preset…
settings-preset-user-name = User preset name
settings-preset-save = Save
settings-preset-saved = Saved as the user preset { $name }.
settings-preset-not-saved = Could not save { $name }: { $error }
settings-preset-no-source = Could not read the source: { $error }
settings-preset-up-to-date = This copy already matches its source.
settings-preset-preview = Updating from the source changes:
settings-preset-field = Field
settings-preset-project = This Project
settings-preset-source = Source
settings-preset-confirm = Update on Apply
settings-preset-note = Updating a copy changes only the preset Output Jobs use. Board Setup has its own rules.
settings-libraries-note = Folders relative to the Project folder, so they move with it.
settings-library-path = Library folder { $number }
settings-library-remove = Remove Library folder { $number }
settings-library-add-path = Add Path
settings-library-new = New Library
settings-library-new-name = New Library name
settings-library-created = Created { $path }; Apply to add it.
settings-library-blank = Give a folder.
settings-library-invalid = { $reason }
settings-library-duplicate = { $path } is listed already.

## The Preferences dialog (#96). Setting labels are pref-<key> with . and _ written as -;
## a setting without one shows its schema description.
pref-title = Preferences
pref-search = Search settings
pref-search-hint = Search settings…
pref-search-none = Nothing matches.
pref-live = Changes apply at once.
pref-close = Close
pref-reset = Reset
pref-reset-named = Reset { $setting } to default
pref-remove = Remove
pref-remove-named = Remove { $item }
pref-add = Add
pref-add-folder = Add Folder…
pref-unlimited = Unlimited
pref-not-a-count = Give a whole number of at least 1, or leave it blank.
pref-not-a-number = Give a number.
pref-blank-path = Give a folder.
pref-duplicate-path = { $path } is listed already.
pref-list-count = { $count ->
    [one] 1 item
   *[other] { $count } items
}
pref-page-general = General
pref-page-schematic = Schematic
pref-page-pcb = PCB
pref-page-display = Display
pref-page-libraries = Libraries
pref-page-fab-presets = Fab Presets
pref-page-agents = Agents
pref-page-keyboard-shortcuts = Keyboard Shortcuts
pref-general-theme = Theme
pref-general-theme-follow-os = Follow the system
pref-general-theme-dark = Dark
pref-general-theme-light = Light
pref-general-ui-scale = UI scale
pref-general-reopen-last-project = Reopen the last Project on start
pref-general-undo-depth = Undo depth
pref-general-default-layout = Default Saved Layout for new Projects
pref-schematic-default-units = Default units
pref-schematic-default-units-mm = mm
pref-schematic-default-units-mil = mil
pref-schematic-repeat-placement = Repeat placement
pref-schematic-electrical-snap = Electrical snap
pref-schematic-auto-pan = Auto-pan at the canvas edge
pref-pcb-default-units = Default units
pref-pcb-default-units-mm = mm
pref-pcb-default-units-mil = mil
pref-pcb-repeat-placement = Repeat placement
pref-pcb-electrical-snap = Electrical snap
pref-pcb-auto-pan = Auto-pan at the canvas edge
pref-display-cursor = Cursor
pref-display-cursor-small = Small crosshair
pref-display-cursor-full-screen = Full-screen crosshair
pref-display-inactive-layer-mode = Inactive Layers in a new view
pref-display-inactive-layer-mode-normal = Normal
pref-display-inactive-layer-mode-dimmed = Dimmed
pref-display-inactive-layer-mode-hidden = Hidden
pref-display-layer-colors-dark = Layer colours, dark theme
pref-display-layer-colors-light = Layer colours, light theme
pref-libraries-paths = User Library folders
pref-libraries-intro = Folders of your own Libraries, searched in every Project.
pref-libraries-none = No folders yet.
pref-library-path = Library folder
pref-agents-bridge = Let agents connect
pref-agents-none-connected = None
pref-forget-named = Forget { $agent }
pref-fab-presets-intro = Your Fab Presets, which new Projects can start from. Save one from a Project's settings with "Save as User Preset…".
pref-fab-presets-none = You have no Fab Presets of your own yet.
pref-fab-preset-name = Name of { $name }
pref-delete = Delete
pref-delete-named = Delete { $item }
shortcuts-search = Search shortcuts
shortcuts-search-hint = Search Commands or keys…
shortcuts-open-folder = Open folder
shortcuts-add = Add Key
shortcuts-add-named = Add a key for { $command }
shortcuts-remove-named = Remove { $key } from { $command }
shortcuts-press-key = Press the key for { $command } (Escape cancels).
shortcuts-recorded = { $key } for { $command }
shortcuts-context = Applies in
shortcuts-context-global = Everywhere
shortcuts-assign = Assign
shortcuts-conflict = { $key } runs { $command } in { $context } now; assigning replaces it there.
shortcuts-no-key = Press a key first.
shortcuts-no-folder = There is no per-user folder.
shortcuts-write-failed = Could not save { $file }: { $error }
# The form generated from a Command's params, and the Command Palette.
form-run = Run
form-fix-fields = Fix the flagged fields first.
form-required = Required
form-whole-number = Enter a whole number
form-bad-json = Not valid JSON: { $error }
form-optional = { $label } (optional)
form-default = (default)
form-yes = Yes
form-no = No
form-choose = Choose…
palette-title = Command Palette
palette-hint = Type a Command…
palette-nothing = No Command matches.

## Routing (pcb::route)
tool-route = Route
tool-drag-track = Drag Track
route-blocked = Too close to other copper: { $actual } where { $required } is needed
route-keepout = That would put copper in a Keepout

# Library element editors (shared) and the Library check.
element-read-only-title = { $name } (read-only)
library-check-line = { $element }: { $text }
library-check-clean = Library check: no problems in { $element }
violation-off-grid-pin = Pin { $pin } is off the { $grid } grid
violation-duplicate-pin-number = Pin number { $number } is used more than once
violation-pin-without-pad = Pin { $pin } has no Pad in { $footprint }
violation-unmapped-pad = Pad { $pad } of { $footprint } is mapped to no Pin
violation-missing-courtyard = The Footprint has no courtyard
violation-broken-reference = Broken { $kind } reference { $reference }

# Import part and the Footprint generators.
library-target = Library
library-target-none = Choose a Library
library-target-missing = No Library you can write to is registered: add one in Preferences or Project Settings.
import-title = Import Part
import-intro = One symbol file and one footprint file from a vendor download become a Symbol, a Footprint and a Part in the Library.
import-symbol-file = Symbol file
import-footprint-file = Footprint file
import-symbol-title = Choose the symbol file
import-symbol-filter = Symbol library
import-footprint-title = Choose the footprint file
import-footprint-filter = Footprint
import-browse = Browse…
import-browse-for = Browse for the { $what }
import-name = Part name
import-name-hint = The symbol's name
import-symbol = Symbol
import-symbol-hint = The only one in the file
import-run = Import
import-needs = Choose both files and a Library first.
import-notice-title = Vendor Parts
notice-ok = OK
generate-preview = Preview
generate-preview-incomplete = Fill in the form to see the land pattern.
generate-preview-pads = { $count } Pads
generate-run = Generate

# The Part editor.
part-read-only = Read-only: this Part is in the starter Library.
part-imported-from = Imported from { $service }; it stays under that service's terms.
part-name = Name
part-symbol = Symbol
part-footprints = Alternate Footprints
part-default-footprint = { $name } (default)
part-none = None
part-missing = Not found
part-not-found = { $uuid } is in no registered Library.
part-needs-symbol = A Standard Part needs a Symbol.
part-filter = Filter
part-edit-symbol = Edit Symbol
part-edit = Edit
part-edit-footprint = Edit { $name }
part-move-up = Up
part-move-down = Down
part-remove = Remove
part-add-footprint = Add Footprint…
part-no-footprints = No Footprints yet: a Standard Part needs at least one.
part-pin-map = Pin-Pad Map
part-pin-map-empty = Pick a Symbol to map its Pins to Pads.
part-pin-map-hint = Each Pin connects to the Pads with its own number unless you list others.
part-pin = Pin
part-pin-name = Name
part-pads = Pads
part-pads-of = Pads of Pin { $pin }
part-overridden = overridden
part-unmapped-pads = Pads that no Pin reaches:
part-pad-unconnected = { $pad } unconnected
part-fields = Fields
part-field-value = Value
part-field-description = Description
part-field-datasheet = Datasheet
part-field-manufacturer = Manufacturer
part-field-mpn = MPN
part-field-keywords = Keywords
part-field-name = Name of field { $n }
part-field-of = { $name }
part-remove-field = Remove { $name }
part-add-field = Add Field

# The Symbol editor.
symbol-read-only = Read-only: this Symbol is in the starter Library.
error-edit-in-library-nothing = Select a placed Component to edit it in its Library.
symbol-unit-tab = Unit { $unit }
symbol-unit-add = Add Unit
symbol-text-hint = Text; Enter places it
pin-table-number = Number
pin-table-name = Name
pin-table-type = Type
pin-table-x = X (mil)
pin-table-y = Y (mil)
pin-table-rotation = Rotation
pin-table-length = Length (mil)
pin-table-name-visible = Name shown
pin-table-number-visible = Number shown
pin-table-paste = Paste Pins
pin-table-paste-hint = Paste rows here: number, name, type, x, y, rotation, length (bare numbers are mil)
pin-table-paste-error = Pasted Pins: { $error }
pin-table-paste-empty = there are no rows to paste

# The Footprint editor.
footprint-read-only = Read-only: this Footprint is in the starter Library.
footprint-save = Save
footprint-check = Library Check
footprint-pad-tool = Pad Tool
footprint-drawing = Drawing
footprint-layer = Layer
footprint-pad-kind = Kind
footprint-pad-shape = Shape
footprint-pad-number = Number
footprint-pad-width = Width
footprint-pad-height = Height
footprint-hole-width = Hole width
footprint-hole-height = Hole height
footprint-mask-margin = Mask margin
footprint-paste-margin = Paste margin
footprint-rotation = Rotation
footprint-stroke-width = Stroke width
footprint-text-size = Size
footprint-text-visible = Visible
footprint-selected-pad = Pad
footprint-selected-graphic = Graphic
footprint-designator-text = Designator text
footprint-value-text = Value text
footprint-pad-smd = SMD
footprint-pad-pth = Plated through-hole
footprint-pad-npth = Non-plated hole
footprint-shape-circle = Circle
footprint-shape-rect = Rectangle
footprint-shape-rounded-rect = Rounded rectangle
footprint-shape-oval = Oval
footprint-layer-silkscreen-top = Silkscreen top
footprint-layer-silkscreen-bottom = Silkscreen bottom
footprint-layer-courtyard-top = Courtyard top
footprint-layer-courtyard-bottom = Courtyard bottom
footprint-layer-assembly-top = Assembly top
footprint-layer-assembly-bottom = Assembly bottom

## Board Setup

board-setup-title = Board Setup
board-setup-rules = Rules
board-setup-page-clearance = Clearance
board-setup-page-track-width = Track Width
board-setup-page-via = Via
board-setup-page-hole-size = Hole Size
board-setup-page-annular-ring = Annular Ring
board-setup-page-hole-spacing = Hole Spacing
board-setup-page-edge-clearance = Edge Clearance
board-setup-page-net-classes = Net Classes
board-setup-page-drc = DRC
board-setup-clearance = Clearance
board-setup-track-width = Track width
board-setup-via-diameter = Via diameter
board-setup-via-hole = Via hole
board-setup-pad-hole = Pad hole
board-setup-annular-ring = Annular ring
board-setup-hole-to-hole = Hole-to-hole spacing
board-setup-edge-clearance = Edge clearance
board-setup-preferred = Preferred { $rule }
board-setup-maximum = Maximum { $rule }
board-setup-board-wide = Board-wide minimum { $rule } (mm)
board-setup-board-wide-preferred = Board-wide preferred { $rule } (mm)
board-setup-board-wide-maximum = Board-wide maximum { $rule } (mm)
board-setup-overrides = Net Class overrides (mm)
board-setup-override = { $class }: { $rule }
board-setup-overrides-note = Leave an override empty to use the board-wide value. Where a Net is in several Net Classes, the highest-ranked one that sets a value wins, and where two Nets meet, the larger clearance applies.
board-setup-no-overrides = Net Classes do not override this rule.
board-setup-net-class = Net Class
board-setup-severity = DRC severity of { $check }:
board-setup-severity-choice = { $check }: { $severity }
board-setup-units = Lengths are in mm unless you give a unit, such as 8mil.
board-setup-fab-preset = Fab Preset rules:
board-setup-apply-preset = Apply { $preset } Rules…
board-setup-preset-preview = Applying the { $preset } rules changes:
board-setup-preset-same = The rules already match { $preset }.
board-setup-rule = Rule
board-setup-now = Now
board-setup-preset = Preset
board-setup-use-preset = Use These Rules
board-setup-no-pcb = The Project has no PCB Document.
board-setup-negative = A length cannot be negative.
board-setup-not-a-length = Not a length. Give mm, such as 0.2, or a unit, such as 8mil.
board-setup-required = Give a length.
board-setup-new-class = Class { $number }
board-setup-class-blank = Give the Net Class a name.
board-setup-class-duplicate = Another Net Class is called { $name }.
board-setup-class-name = Net Class { $number } name
board-setup-delete = Delete
board-setup-delete-class = Delete Net Class { $number }
board-setup-default-class = The Default Net Class cannot be deleted.
board-setup-add-class = Add Net Class
board-setup-nets = Nets
board-setup-filter = Filter Nets
board-setup-no-nets = The board has no Nets yet.
board-setup-class-of = Net Class of { $net }
board-setup-default-class-name = Default

## Output Job dialog and job runs

output-job-title = Output Job: { $job }
output-job-page = Job
output-job-name = Name
output-job-pcb = PCB Document
output-job-pcb-missing = (missing)
output-job-fab-preset = Fab Preset
output-job-outputs = Outputs
output-job-gerber = Gerber X2
output-job-drill = Excellon drill
output-job-bom = CSV BOM
output-job-cpl = CSV CPL (pick and place)
output-job-destination = Destination folder
output-job-destination-default = Empty: { $folder }
output-job-name-blank = Give the job a name.
output-job-name-taken = Another job is already called { $name }.
output-job-broken = This job's PCB Document is gone. Pick another to repair it.
output-job-gone = This Output Job was deleted.
output-job-destination-invalid = Give a /-separated folder inside the Project folder.
output-job-run = Run
output-job-run-hint = Applies your changes, then runs the job.
output-job-broken-run = Output Job { $job } names a PCB Document that is gone: edit it to point it at another.
output-job-broken-label = { $job } (broken)
output-job-starting = Starting
output-job-running = Running { $job }
output-job-cancelling = Cancelling…
output-job-done = { $job } finished
output-job-blocked = { $job } did not run
output-job-cancelled = { $job } was cancelled
output-job-failed = { $job } failed
output-job-written = Wrote { $count } { $count ->
    [one] file
   *[other] files
} and their zip to { $folder }.
output-job-nothing-written = Nothing was written.
output-job-blockers = Blocking problems
output-job-warnings = Warnings
output-job-show-zip = Show Zip
output-job-close = Close
output-job-busy = Another Output Job is running.
message-output-job = { $job }: { $text }
violation-board-outline-open = The Board Outline is missing or not closed
violation-missing-footprint = { $designator }'s Footprint is missing from the PCB
violation-unassigned-footprint = { $designator } has no Footprint, so it is not on the PCB: assign one and Sync
violation-erc-errors = ERC finds { $count } { $count ->
    [one] error
   *[other] errors
}
violation-drc-errors = DRC finds { $count } { $count ->
    [one] error
   *[other] errors
}
violation-rule-below-preset = Board { $rule } { $board } is below { $preset }'s minimum { $minimum }
violation-missing-font = Font “{ $font }” is not installed

# Schematic: Net highlighting, graphics Tools and dialogs (Find, Select Similar,
# Re-annotate, Update from Library, Export PDF).
schematic-text-preview = Text
schematic-text-title = Text
schematic-text-words = Text
schematic-text-place = Place Text
find-title = Find
find-text = Find what
find-hint = Designator, value or Net name
find-search = Find
find-exact = Whole name only
find-nothing = Nothing found.
find-field-designator = Designator
find-field-value = value
find-field-net = Net
find-row = { $text } ({ $field }) on { $sheet }
similar-title = Select Similar to { $designator }
similar-match = Select the Components that match on:
similar-part = Library Part
similar-value = Value ({ $value })
similar-footprint = Footprint
similar-prefix = Designator prefix
similar-unit = Symbol Unit
similar-component-type = Component Type
similar-not-fitted = Not Fitted
similar-field = { $name } ({ $value })
similar-scope-sheet = This sheet
similar-scope-project = Every sheet
similar-select = Select
similar-pick-a-field = Tick at least one field to match on.
similar-needs-unit = Select a Component first: Select Similar compares with it.
reannotate-title = Re-annotate
reannotate-order = Order
reannotate-across-then-down = Across, then down
reannotate-across-then-up = Across, then up
reannotate-down-then-across = Down, then across
reannotate-up-then-across = Up, then across
reannotate-board-position = By board position
reannotate-scope = Scope
reannotate-scope-project = Project
reannotate-scope-sheet = This sheet
reannotate-scope-selection = Selection
reannotate-preview = Preview
reannotate-row = { $old } » { $new }
reannotate-lock = Lock
reannotate-locked = Locked Designators
reannotate-unlock = Unlock
reannotate-no-changes = Nothing would be renumbered.
reannotate-nothing-selected = Select the Components to renumber first.
reannotate-apply = Re-annotate
library-update-title = Update from Library
library-update-none = Every Component is up to date with its Library Part.
library-update-blocked = A placed Unit is gone from the Symbol: delete it to update this Component.
library-update-apply = Update
library-update-none-ticked = Tick the Components to update.
library-update-kept = { " " }(this Component's own value is kept)
library-update-name = Part renamed: { $from } » { $to }
library-update-component-type = Component Type: { $from } » { $to }{ $kept }
library-update-field = { $name }: “{ $from }” » “{ $to }”{ $kept }
library-update-pin-map = Pin-pad map changed
library-update-symbol = Symbol { $name } changed
library-update-unit-removed = Unit { $unit } is gone from the Symbol
library-update-footprint = Footprint { $name }: { $change }
library-update-footprint-order = Footprint alternates reordered
library-update-footprint-choice = The chosen Footprint is no longer an alternate: it moves to the new default
export-pdf-title = Export Schematic PDF
export-pdf-filter = PDF
export-pdf-done = Exported { $pages ->
    [one] { $pages } sheet
   *[other] { $pages } sheets
} to { $path }
## The Change List dialog (Update PCB from Schematic)
change-list-title = Update PCB from Schematic
change-list-page = Change List
change-list-in-sync = The PCB is in sync with the schematic.
change-list-summary = { $ticked } of { $count } changes ticked. Unticked changes show up again next time.
change-list-group = { $group } ({ $count })
change-list-tick-all = All
change-list-tick-group = Tick all: { $group }
change-list-group-components = Components
change-list-group-footprints = Footprints
change-list-group-component-data = Component data
change-list-group-nets = Nets
change-list-group-pad-nets = Pad Nets
change-add-component = Add { $designator } ({ $footprint })
change-remove-component = Remove { $designator }
change-footprint = { $designator }: Footprint { $from } to { $to }
change-designator = Designator { $from } to { $to }
change-fields = { $designator }: { $fields }
change-not-fitted-on = { $designator }: Not Fitted
change-not-fitted-off = { $designator }: Fitted
change-add-net = Add Net { $net }
change-rename-net = Rename Net { $from } to { $to }
change-remove-net = Remove Net { $net }
change-merge-net = Merge Net { $net } into { $into }
change-pad-net = { $designator }.{ $pad }: { $from } to { $to }
change-no-net = (no Net)
status-out-of-sync = out of sync: { $count ->
    [one] 1 change
   *[other] { $count } changes
}
status-out-of-sync-hint = Update PCB from Schematic…
tab-out-of-sync-badge = { $count }
tab-out-of-sync = { $tab }, out of sync: { $count ->
    [one] 1 change
   *[other] { $count } changes
}
