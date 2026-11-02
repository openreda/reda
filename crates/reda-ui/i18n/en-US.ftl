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
settings-erc-matrix = Pin-type matrix (fixed): E is a Pin conflict, W is a Mixed pin conflict.
severity-error = Error
severity-warning = Warning
severity-off = Off
erc-unconnected-input = Unconnected input
erc-unconnected-pin = Unconnected pin
erc-pin-conflict = Pin conflict
erc-pin-conflict-mixed = Mixed pin conflict
erc-undriven-power = Undriven power
erc-single-pin-net = Single-pin Net
erc-dangling-wire = Dangling Wire
erc-unattached-label = Unattached label
erc-no-connect-connected = Connected No-Connect
erc-multiple-net-names = Several Net names
erc-single-sheet-port = Port on one sheet only
erc-duplicate-designator = Duplicate Designator
erc-unplaced-units = Unplaced Units
erc-unit-data-mismatch = Unit data mismatch
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
