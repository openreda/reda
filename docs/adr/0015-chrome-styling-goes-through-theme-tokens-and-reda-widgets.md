# Chrome styling goes through theme tokens and REDA widgets

The chrome's look needs different recipes per component (a primary button, a focus ring, a text field unlike a button), in light and dark and in two Densities, and stock egui shares one set of visuals across all of them. REDA keeps every colour, space, radius, text style, shadow and duration in one `Tokens` value in `reda-ui`'s `theme` module, generates egui's `Style` from it, and builds each component as a REDA widget in the `widgets` module. Code outside those two modules reads `ui.tokens()` and uses REDA widgets; it does not read `ui.visuals()` or `ui.spacing()`, construct a colour, pass a literal size, or call a stock egui widget that has a REDA one. A source-scanning test enforces this.

## Considered Options

- **egui's class-based widget themes (the `experimental` feature):** call sites would stay as they are, but it covers only Button, TextEdit, Checkbox and Separator, cannot animate a widget, and egui documents it as open to breaking changes or removal.
- **Keep `ui.visuals()` as a second source beside the tokens:** less to migrate, but egui's slots stay borrowed for other meanings (`selection.stroke` as an accent line) and the rule cannot be checked.
- **Clippy `disallowed_methods`:** precise on method paths but blind to literal arguments such as `add_space(6.0)`, which is where most of the bypassing is.
- **A separate theme crate:** would keep the palette private by crate boundary, but the widgets need `reda-ui`'s icons, translations and Command plumbing.

## Consequences

- Tokens are named by job (`surface.card`, `space.row_gap`), never by value. The raw palette is private to `theme`.
- Each token family follows one setting: colours and shadows follow light/dark, space follows Density, text styles and widths follow the text size and typeface Preferences, and radii, stroke widths, icon sizes and motion are fixed. Control and row heights are computed from line height plus padding, not stored.
- A text style is a whole token (family, weight and size together), because egui installs each weight as its own font family.
- Hover and press on a coloured fill are derived by one overlay function in `theme`, the only colour blending allowed; disabled is one opacity.
- Contrast rules are a table beside the tokens, and a test fails when a colour token appears in no rule.
- The scan works on text, so an alias or a macro can defeat it. It allows zero, arithmetic on token values, and a line marked `// theme-exempt: <reason>`. On-canvas drawing modules are exempt and take their few chrome colours through `theme::canvas_theme`.
- During the migration the test carries a list of files not yet converted. A file off the list must be clean, and a clean file left on the list fails the test.
- Stock widgets REDA does not wrap, and third-party ones such as the file dialog, still get their look from the generated `Style`.
