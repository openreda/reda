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

## Menus

menu-file = File
menu-file-new-project = New Project…
menu-file-open-project = Open Project…
menu-file-save-all = Save All
menu-file-quit = Quit
menu-view = View
menu-view-zoom-fit = Zoom to Fit
menu-view-theme = Theme
theme-follow-os = Follow the System
theme-dark = Dark
theme-light = Light

## Dialogs

dialog-cancel = Cancel
dialog-open-project-title = Open Project
dialog-project-filter = REDA Project
dialog-location-title = Choose Where to Create the Project
new-project-title = New Project
new-project-default-name = Untitled
new-project-name = Name
new-project-location = Location
new-project-browse = Browse…
new-project-create = Create

## Canvas

canvas-description = Schematic canvas: { $document }

## Errors

error-command = { $error }
error-state-file = Could not update the recent Projects list: { $error }

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
