# PROTOTYPE (issue #282): which transitions does the chrome animate?

Throwaway. Never merged. Built on the component-recipes prototype (issue #276).

## Run it

```
git switch prototype/motion
cargo run -p reda-app        # then F10: the gallery opens on page 8, Motion
```

Click everything on the page. The strip at the top:

- **A · Still / B · Crisp / C · Fluid** pick the motion profile (F8 cycles).
- **Animate** off makes every transition happen at once.
- **Speed** 1/4× and 1/10× slow every transition down, to see what it does.
- **Play all** flips every demo at once, and back.

The other gallery keys still work: Shift+F9 light/dark, Ctrl+F9 Density, F9 page.

## The three profiles

All times in ms, in / out. "at once" means no transition.

| Transition | A · Still | B · Crisp | C · Fluid |
| --- | --- | --- | --- |
| Hover fill, scrollbar widening | 90 / 90 linear | at once / 90 linear | 60 / 140 linear |
| Press fill | at once | at once / 90 ease-out | at once / 140 ease-out |
| Check box, radio, toggle on | at once | 90 / 90 ease-out | 200 / 90 spring |
| Switch | 140 / 140 ease-out | 140 / 140 ease-out | 260 / 260 spring |
| Tab accent line | at once | 90 / at once, fades in | 200, glides to the new tab |
| Menu, context menu, combo list | at once | 90 / at once, fade + 4 px | 200 / 90 spring, grows from 94% |
| Tooltip | at once after 500 | 90 / at once after 500 | 140 / 90 after 350 |
| Flyout | at once | 140 / 90, fade + 8 px | 200 / 140, slides from under the strip |
| Dock collapse | at once | 140 / 140 ease-out | 260 / 260 ease-in-out |
| Panel section | at once | 140 / 90 ease-out | 200 / 140 ease-in-out |
| Dialog and scrim | at once | 140 / 90, fade + grows from 97% | 320 / 140 spring, grows from 90% |
| Command Palette | at once | 140 / 90, fade + 4 px drop | 200 / 140, grows out of the command field |
| Selected row | at once | at once | 140, glides to the new row |
| Bars on the canvas | at once | 90 / 90, fade + 4 px | 260 / 140 spring; the on state glides |
| Progress bar | at once | 140 ease-out | 320 ease-in-out |

- **A · Still** is what the component recipes left: hover fills and the switch.
- **B · Crisp** uses only the three durations the recipes fixed (90, 140, 200; it never
  needs 200) and one easing, ease-out. Things leave faster than they arrive. Hover-in,
  press-in and selection are immediate.
- **C · Fluid** needs longer durations (260, 320), a spring, and shared-element movement
  (the accent line, the selected row and the on state travel between items; the Command
  Palette grows out of the command field).

Never animated in any profile: the focus ring and caret, a press arriving, keyboard
selection, what a tab holds, anything being dragged, layout during a window resize, and
the canvas (its zoom, flip and Layer-switch animation is its own Preference).

## Where the code is

- `crates/reda-ui/src/proto/anim.rs`: the profiles and the engine (about 60 lines: a
  start value, a target, a start time, an easing, kept per id in egui's memory).
- `crates/reda-ui/src/proto/pages/motion.rs`: the page and its demos.
- `crates/reda-ui/src/proto/widgets.rs`: hover, press, check, radio, switch and toggle
  read their timing from the profile.
- `crates/reda-ui/tests/motion_gallery.rs`: draws the frames in this folder
  (`REDA_GALLERY_OUT=<dir> cargo test -p reda-ui --test motion_gallery`).

## What building it showed

- Fade, travel and scale of a popup need nothing from egui beyond
  `Ui::with_visual_transform` and `Ui::multiply_opacity`: the popup is painted once at
  its final place and the frame's shapes are moved. No second layout.
- A closing transition means the thing is still painted after it is closed. Here the
  popups are painted by the page, so that is free. egui's own menus and `Area` popups
  stop painting the frame they close, so in the build an animated close needs REDA's own
  popup widget. An animated open does not.
- A spring makes the value pass 1, so opacity and colour mixes have to clamp it.

## The frames

`motion-<profile>-<theme>-0-before`, `-1-at-060ms` (60 ms into Play all), `-3-after`, and
`motion-b-crisp-light-4-tooltip`. Stills cannot show timing: run it.
