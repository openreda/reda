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
