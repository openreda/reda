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
