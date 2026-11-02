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
error-state-file =Could not update the recent Projects list: { $error }
error-recovery = Recovery Snapshots: { $error }
error-project-held = { $project } is already open in another REDA window (process { $pid }); it stays there.

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
