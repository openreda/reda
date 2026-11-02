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
panels-reopen = { $panel } (closed)
floating-window-title = REDA
status-tool-cancelled = { $tool } was cancelled: an { $source } edit changed what it was using.

## The bridge: agents connecting through the control port

bridge-indicator-off = Agents: off
bridge-indicator-on = Agents: on
bridge-indicator-connected = Agents: { $count } connected
bridge-switch = Let agents connect
bridge-connected = Connected now
bridge-allowed = Allowed agents
bridge-none-allowed = None yet
bridge-forget = Forget
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
menu-view-theme = Theme
menu-design = Design
menu-library = Library
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
properties-new-field-name = New field
properties-new-field-value = Value of the new field
properties-add-field = Add Field

## Canvas

canvas-description = Schematic canvas: { $document }

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
settings-library-blank = Give a folder.
settings-library-invalid = { $reason }
settings-library-duplicate = { $path } is listed already.

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
