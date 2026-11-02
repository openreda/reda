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
