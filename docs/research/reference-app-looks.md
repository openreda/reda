# What makes the reference apps look the way they do

Research for [#268](https://github.com/openreda/reda/issues/268), a child of the theme-baseline map [#266](https://github.com/openreda/reda/issues/266). Researched 2026-11-14.

The references are SwiftUI's macOS defaults, the Codex desktop app, the Claude desktop app, T3 Code and BridgeMind. This note takes each look apart, lists what they share and where they split, and ends with three directions a prototype could open with.

## How far to trust each part

Every value below carries one of these marks.

- **[source]**: read from the product's own source code or design document at a pinned commit.
- **[docs]**: stated in the vendor's own documentation.
- **[derived]**: computed by me from a [source] or [docs] value (a colour mix, an OKLCH or palette-name conversion, a contrast ratio). The input is cited; the arithmetic is mine.
- **[screenshot]**: read by eye from an official screenshot. An estimate.
- **[recollection]**: my own memory of the product, not checked against any source in this research. Treat as a guess until someone looks at the running app.

Coverage is uneven, and the gaps matter:

| Reference | What was established | What was not |
| --- | --- | --- |
| T3 Code | Full token set, component recipes, motion, title bar geometry, from the open-source repo. | Nothing material. |
| BridgeMind | A complete written design system and tokens for **BridgeClip**, BridgeMind's open-source desktop app. | **BridgeSpace**, the product the name most likely refers to, could not be examined at all: `bridgemind.ai` and `docs.bridgemind.ai` answer automated requests with HTTP 403, and its source is not public. Whether BridgeSpace looks like BridgeClip is an assumption. BridgeClip is dark-only, so there is no BridgeMind light theme to learn from. |
| Codex desktop app | Default accent, background and foreground in light and dark, default fonts and sizes, the appearance controls, from the official settings docs. | Radii, control heights, shadows, sidebar and tab treatment, title bar, motion. The app is closed source, the public `openai/codex` repo holds only the CLI/TUI, and `openai.com/codex` returned HTTP 403, so no official screenshot was examined. |
| Claude desktop app | Layout and pane model from the official docs; Anthropic's brand palette from Anthropic's own published skill. | Every app-specific visual value. The docs carry no screenshots and no appearance settings, `claude.ai` returned HTTP 403, and the app is closed source. The brand palette is a company palette, not the app's token set. |
| SwiftUI / macOS | Type scale, system colours and the design principles, from the Human Interface Guidelines. | Control heights, corner radii and window radius: the HIG does not publish them for macOS. |

## T3 Code

Source: `pingdotgg/t3code` (MIT) at commit [`dd50613`](https://github.com/pingdotgg/t3code/tree/dd5061364bbf52f5008a16fb22e367c3680bdbd1). An Electron app whose UI is React, Tailwind v4 and shadcn-style components on Base UI ([`components.json`](https://github.com/pingdotgg/t3code/blob/dd5061364bbf52f5008a16fb22e367c3680bdbd1/apps/web/components.json): style `base-mira`, base colour `zinc`, Lucide icons). All tokens are in [`apps/web/src/index.css`](https://github.com/pingdotgg/t3code/blob/dd5061364bbf52f5008a16fb22e367c3680bdbd1/apps/web/src/index.css). Hex values for Tailwind palette names (`zinc-50` and so on) are Tailwind's defaults as I know them, so they are [derived].

**Surfaces.** A pure, hue-free grey ramp. Light uses three close levels; dark is true black with barely lifted cards.

| Token [source] | Light | Dark |
| --- | --- | --- |
| `--background` (canvas, chrome, toolbar) | `zinc-25` = `oklch(99.2% 0 0)`, about `#FDFDFD` | `#000` |
| `--sidebar` | `zinc-50`, `#FAFAFA` | same as card |
| `--card`, `--popover` (raised) | white | `neutral-950` mixed 3% toward white, about `#111111` |
| `--message-surface` (user turn) | `zinc-200`/`zinc-100` mix, about `#EBEBED` | `neutral-950` mixed 8% toward white, about `#1E1E1E` |
| `--accent` (hover fill), `--muted` | `zinc-100` `#F4F4F5`, `zinc-50` | white at 4%, white at 3% |
| `--border` / `--input` | `zinc-200` `#E4E4E7` / `zinc-300` `#D4D4D8` | white at 8% / white at 11% |
| `--foreground` / `--muted-foreground` | `zinc-900` `#18181B` / `zinc-500` `#71717A` | `neutral-100` `#F5F5F5` / `neutral-400` `#A3A3A3` |

The dark-mode comment in the source states the intent: "The canvas is true black ... Borders and hover states provide separation without milky gray fills." In light, the sidebar is the *darker* level and the selected row is white, so selection reads as a raised chip (`--sidebar-row-selected: white`, hover `zinc-25`). In dark, selection is a 7% foreground tint.

**Borders versus shadows.** Borders do the structural work; shadows are tiny. Outline buttons and inputs carry `shadow-xs` at 5% plus a 1px inner highlight (black 4% on the bottom edge in light, white 6% on the top edge in dark). The composer has `0 1px 2px` at 5% in light and `0 14px 32px -18px` at 75% in dark. Only floating things get real depth: menus `0 16px 40px -18px rgb(0 0 0 / 55%)` (80% in dark), dialogs `0 24px 64px -24px` at 65%.

**Glass.** Menus, tooltips, dialogs and pill buttons use a `backdrop-filter`: `--glass-blur` 12px (16px dark), `--glass-opacity` 80%, saturation 1.14 (1.08 dark), with a 1px border of foreground at 10%. Each has an `@supports not` fallback to the solid surface.

**Radii.** `--radius` and `--control-radius` are both `0.5rem` (8px). The scale is `sm` 4, `md` 6, `lg` 8, `xl` 12, `2xl` 16, `3xl` 20. Controls and sidebar rows are 8; compact controls and tooltips 6; menu items 4 inside an 8 menu; dialogs 16. Pills exist only as an explicit variant.

**Type.** `-apple-system, BlinkMacSystemFont, "Inter Variable", "Segoe UI", system-ui, sans-serif`: the system face first, Inter as the cross-platform fallback. Mono is `ui-monospace, "SF Mono", Menlo, Consolas, ...`. Body UI text is 14px (`text-sm`), compact controls 12px, metadata 11px (`--text-2xs`) and 10px (`--text-3xs`). Weights: 500 for buttons and the selected row, 600 for dialog titles (20px), regular elsewhere.

**Control sizes** [source], desktop breakpoint: default button 32px high with about 11px side padding; `sm` 28; `xs` and `compact` 24 and 28; `lg` 36; icon buttons 24 to 40. Inputs 30px (26 `sm`, 28 compact, 34 `lg`). Sidebar rows 32px with 10px inset; sidebar 256px wide, 48px as an icon rail. Menu items at least 32px on touch. Icons 16px in controls, 14px in compact ones.

**Colour use.** One accent, `--primary: oklch(0.488 0.217 264)` in light and `oklch(0.571 0.21 264)` in dark: a saturated blue-indigo, roughly `#2450D8` and `#3F6FF2` [derived, approximate]. It appears on the default button, the focus ring, switches when on, and little else. Everything else is grey. Status colours are the Tailwind 500s for fills (red, amber, emerald, blue) with 700 for text in light and 400 in dark; status surfaces are the colour at 8% (16% dark).

**Sidebar and tabs.** A flat list sidebar with no cards; rows are 8px-radius fills. Icons in the sidebar are muted (85% of the muted foreground). The official screenshot shows no tab strip: navigation is the sidebar plus a breadcrumb in the top bar [screenshot].

**Title bar.** Custom, 52px (`--workspace-topbar-height`), collapsing to `max(40px, titlebar-area-height)` under Window Controls Overlay. Title-bar controls are 28px with 12px gaps. The bar is the same colour as the canvas with no border, so it is not visibly a bar at all; content fades under it [source, screenshot].

**Motion.** Sparse and short. Buttons scale to 0.97 on press. Switches 200ms. Tooltips and menus scale from 0.98 with opacity. A decelerating `--ease-drawer: cubic-bezier(0.32, 0.72, 0, 1)` for things that slide in. `prefers-reduced-motion` is honoured, and looping indicators are stepped to save frames.

## BridgeMind

**Not established: BridgeSpace.** Third-party listings describe BridgeSpace as a desktop terminal workspace with multi-pane layouts and "12 customizable themes", but I could not reach a first-party page or screenshot, so nothing here describes BridgeSpace. Do not read the next paragraphs as a description of it.

**Established: BridgeClip.** `bridge-mind/bridgeclip` (MIT, Electron + React + Tailwind) at commit [`b996820`](https://github.com/bridge-mind/bridgeclip/tree/b996820ef80fa324a3ad233bef1aac5cc6936804) ships a written design system, [`DESIGN.md`](https://github.com/bridge-mind/bridgeclip/blob/b996820ef80fa324a3ad233bef1aac5cc6936804/DESIGN.md), titled "Flat Glass", with tokens in [`globals.css`](https://github.com/bridge-mind/bridgeclip/blob/b996820ef80fa324a3ad233bef1aac5cc6936804/src/renderer/globals.css) and [`tailwind.config.ts`](https://github.com/bridge-mind/bridgeclip/blob/b996820ef80fa324a3ad233bef1aac5cc6936804/tailwind.config.ts). It is the best first-party evidence of BridgeMind's house style. It is **dark only**. All values below are [source].

**Surfaces.** Cool, blue-tinted near-black with four solid levels and translucent white layers on top.

- Solids: `canvas` `#07080C`, `surface` `#0E0F15`, `raised` `#16181F`, `overlay` `#1A1C24`.
- Materials: panel `glass` is white at 4.5% with a 1px inset rim of white at 8%; nested `glass-tile` is white at 3.3% with a white 8% border and a 1px top highlight; `glass-well` (inputs) is *black* at 26%, recessed with an inner shadow; `glass-thick` (dialogs, popovers, sticky bars) is `rgb(25 27 36 / 0.8)` with `blur(40px) saturate(170%)`.
- Fills: hover white 5%, hover-strong 8%, selected 11%. Lines: white 9%, strong 16%.
- Text: `ink` `#F7F8FC`, `ink-muted` `#AAAEBC`, `ink-subtle` `#7C8190`, `ink-faint` `#545967`.

The sidebar has no background of its own: it shares the canvas with the page and is separated by one white-6% hairline.

**Borders versus shadows.** Both, layered. Every surface has a crisp 1px rim, and floating surfaces add a large, soft, black-only shadow: panel `0 1px 2px` at 25% plus `0 18px 48px -24px` at 60%; popups `0 40px 100px -24px` at 80%. The document forbids gradients and coloured glows: "Only black shadows blur."

**Radii.** Larger than T3's and explicitly concentric (inner radius = outer radius minus padding). Scale: 6, 8, 10, 12, 14, 18. Panels and dialogs 18; cards and rows 14; inputs and small tiles 12. **Controls are capsules**: buttons, badges, chips, segmented controls and switches are fully rounded.

**Type.** Geist and Geist Mono, bundled (SIL OFL) "so the app renders identically offline". UI text is 13px on an 18px line. Scale: 11/14, 12/16, 13/18, 14/20, 16/22, 20/26, 24/30. Panel titles 13px semibold; page titles 20px semibold; uppercase 11px semibold "eyebrow" group labels with 0.1em tracking. Changing numbers use the mono face with tabular figures. OpenType features `cv11` and `ss01` are on.

**Control sizes.** Buttons and inputs share heights: 28, 32, 40. Sidebar 200px, or a 72px icon rail. Compact by rule: panels pad 16px, stacks gap 12px, and the document says not to reintroduce 24px padding.

**Colour use.** The strictest of the set. One accent, `#1A5FDF`, with white text (5.6:1 [derived]), reserved for "the primary action, chosen options, live progress, the active nav icon and the drop target. Selection elsewhere is a brighter pane of glass, not blue." Status hues only for status: success `#4ADE80`, warning `#FBBF24`, danger `#FF6B6E`. Focus ring is the accent at 75%, 2px, offset 2px.

**Title bar.** No drawn bar. A solid 40px strip in the canvas colour that content scrolls under; the whole backdrop is a drag region and macOS traffic lights sit in the sidebar's top 40px.

**Motion.** One easing, `cubic-bezier(0.16, 1, 0.3, 1)`. Hover and state changes 150 to 200ms; menus 140ms from scale 0.97; dialogs 360ms from scale 0.97 with an 8px rise; pages 320ms with a 6px rise. A slight-overshoot spring is used only for the switch knob. "Nothing loops just to decorate." Reduced motion and reduced transparency are both honoured.

## Codex desktop app

Source: the official settings reference, [developers.openai.com/codex/app/settings](https://developers.openai.com/codex/app/settings) (currently served from `learn.chatgpt.com/docs/reference/settings`). The page renders the app's own Appearance panel with its defaults.

| Default [docs] | Light | Dark |
| --- | --- | --- |
| Accent | `#0285FF` | `#339CFF` |
| Background | `#FFFFFF` | `#181818` |
| Foreground | `#0D0D0D` | `#FFFFFF` |
| Contrast | 45 | 60 |

- A theme is only three colours plus a contrast number: the rest of the palette (borders, hover fills, muted text, the sidebar) is derived from them. The docs do not explain the contrast scale. [docs]
- UI font: a system stack beginning `-apple-system, BlinkM...` (truncated in the panel). Code font begins `ui-monospace, "SFMo...`. UI font size 14px; code font size 14px. [docs]
- A "Translucent sidebar" toggle exists; its default is not stated. [docs]
- Themes can be imported and shared as strings. The feature shipped in app version 26.312 on 2026-03-12 according to the [changelog](https://developers.openai.com/codex/changelog). [docs]
- The accent is a bright sky blue that does not carry text: `#0285FF` on white is 3.6:1, and white on `#339CFF` is 2.9:1 [derived]. Used as a fill behind white text or as link-coloured text on white, neither default meets 4.5:1. How the app actually uses it was not verified.
- Dark is a soft neutral charcoal, not black: `#181818` against T3's `#000` and BridgeClip's `#07080C`.

Not established: radii, control heights, shadows, sidebar and tab treatment, title bar, motion. My [recollection] is a neutral, borderless look with a slightly tinted sidebar, a large rounded composer and mid-sized radii, but none of that was checked.

## Claude desktop app

**Layout** [docs], from [code.claude.com/docs/en/desktop](https://code.claude.com/docs/en/desktop) and the [redesign post](https://claude.com/blog/claude-code-desktop-redesign): a sessions sidebar with hover-revealed group and sort controls; a workspace of panes (chat, diff, browser, terminal, file editor, plan, tasks) that are dragged by their headers into a grid and resized at their edges; a title bar that holds icon buttons for opening panes, with overflow moving into a `⋮` menu when the window narrows; a "session rail" of cards beside the chat; panes that minimise into a corner or pop out into their own windows. That is the closest of the references to REDA's Docks-and-tabs shell, and it puts pane controls in the title bar rather than in a separate toolbar.

**Brand palette** [source: Anthropic's own [`brand-guidelines` skill](https://github.com/anthropics/skills/blob/main/skills/brand-guidelines/SKILL.md)]: dark `#141413`, light `#FAF9F5`, mid grey `#B0AEA5`, light grey `#E8E6DC`; accents orange `#D97757`, blue `#6A9BCC`, green `#788C5D`. Every neutral is warm (yellow-leaning), and the accent is a muted terracotta. BridgeClip's design document independently records Claude's mark as terracotta `#D97757`.

These are company brand colours. That the desktop app's surfaces use exactly these values is **not verified**. Useful facts that follow from the palette regardless [derived]: `#D97757` is 3.0:1 on `#FAF9F5` and 3.1:1 under white text, so it cannot carry text in light mode at AA; dark ink `#141413` on it is 5.9:1.

Not established: the app's actual surface levels, radii, control heights, typefaces, shadows, motion. My [recollection]: cream surfaces in light and a warm dark grey (not black) in dark, a slightly darker sidebar, a serif face for assistant prose with a sans for the chrome, generous radii on the composer and cards, hairline warm borders, and very sparing use of the terracotta. None of that was checked here.

## SwiftUI and macOS defaults

Source: Apple's Human Interface Guidelines, updated for Liquid Glass (macOS 26) in 2025.

**Type** [docs, [Typography](https://developer.apple.com/design/human-interface-guidelines/typography)]. The macOS default text size is 13pt and the minimum 10pt. The built-in styles:

| Style | Weight | Size | Line height | Emphasised |
| --- | --- | --- | --- | --- |
| Large Title | Regular | 26 | 32 | Bold |
| Title 1 | Regular | 22 | 26 | Bold |
| Title 2 | Regular | 17 | 22 | Bold |
| Title 3 | Regular | 15 | 20 | Semibold |
| Headline | Bold | 13 | 16 | Heavy |
| Body | Regular | 13 | 16 | Semibold |
| Callout | Regular | 12 | 15 | Semibold |
| Subheadline | Regular | 11 | 14 | Semibold |
| Footnote | Regular | 10 | 13 | Semibold |
| Caption 1 | Regular | 10 | 13 | Medium |

The pattern worth copying: one size (13) for nearly everything, hierarchy from weight and colour rather than size, and a tight line height (about 1.23).

**Colour** [docs, [Color](https://developer.apple.com/design/human-interface-guidelines/color)]. System blue is `rgb(0,136,255)` in light and `rgb(0,145,255)` in dark, with increased-contrast variants `rgb(30,110,244)` and `rgb(92,184,255)`. Codex's default accents (`#0285FF`, `#339CFF`) are within a few units of system blue, which is why that app reads as "native". Text comes in four semantic levels (label, secondary, tertiary, quaternary) rather than named greys. Apple's own default blue is 3.5:1 on white [derived], so the platform itself does not hold accent-coloured text to 4.5:1; REDA does.

**Principles** [docs, [Materials](https://developer.apple.com/design/human-interface-guidelines/materials), [Toolbars](https://developer.apple.com/design/human-interface-guidelines/toolbars), [Sidebars](https://developer.apple.com/design/human-interface-guidelines/sidebars), [Buttons](https://developer.apple.com/design/human-interface-guidelines/buttons), [Windows](https://developer.apple.com/design/human-interface-guidelines/windows), [Motion](https://developer.apple.com/design/human-interface-guidelines/motion)]:

- Controls and navigation form a distinct layer floating above content; the content layer stays plain. Translucency belongs to the control layer only and is used sparingly.
- Corner radii are concentric: a control's radius follows from its container's.
- Toolbars: "Reduce the use of toolbar backgrounds and tinted controls"; toolbar items have no bezel; the window title can sit inline with the controls; exactly one prominent (tinted) action, on the trailing side.
- Buttons: one or two prominent buttons per view. The accent marks the primary action; red marks destructive ones.
- Sidebars: icons take the accent colour by default; fixed colours are allowed "if you use them sparingly" to clarify meaning.
- Inactive windows lose colour in the title bar controls and lose vibrancy, so they recede.
- Motion: "brief and precise" feedback, optional, and never the only carrier of information.

Not published by the HIG: macOS control heights, control and window corner radii, shadow values. My [recollection], unverified: before macOS 26 a regular control was about 22pt high with a 5 to 6pt radius and a window about 10pt; macOS 26 made controls taller and rounder (capsule-leaning) and windows markedly rounder.

## What they share

1. **Neutral surfaces, few levels.** Three or four surface levels, a few percent apart. No mid-grey panels, no gradients. Dark themes are built by adding white at 3 to 11% over a dark base rather than by picking separate greys (T3, BridgeClip).
2. **Hairlines over boxes.** Structure comes from 1px borders at very low contrast: about 8 to 10% of the foreground in dark, about `#E4E4E7` in light. Shadows are reserved for things that float (menus, dialogs, popovers), where they are large, soft, offset downward and black.
3. **A 1px inner highlight** on raised controls (white at 5 to 16% on the top edge) is the detail that makes flat controls look finished. T3 and BridgeClip both do it.
4. **One accent, used rarely.** Primary action, focus ring, switch-on, chosen option, progress. Hover and selection are neutral fills, not accent fills. Status hues appear only for status.
5. **Small type, weight for hierarchy.** UI text is 13 or 14px everywhere (13 macOS and BridgeClip, 14 T3 and Codex), secondary text 11 to 12px, medium (500) for controls and selection, semibold (600) for titles. Size changes are rare.
6. **System-like sans.** SF via the system stack, with Inter as the fallback (T3, Codex), or Geist bundled (BridgeClip). All are neutral grotesques with tabular figures available.
7. **Compact controls.** 28 and 32px are the working heights in both open-source apps; 24px for dense rows; 36 to 40px only for a primary call to action.
8. **The title bar is not a bar.** Same colour as the window, no border under it, 40 to 52px high, holding the sidebar toggle, a title or breadcrumb, and pane toggles. Documented for T3, BridgeClip, Claude (pane buttons in the title bar) and in the HIG.
9. **Quiet sidebars.** Flat lists, 32px rows, 8px or pill selection fill, muted icons, small uppercase or muted group labels, collapsible to an icon rail.
10. **Short, decelerating motion.** 140 to 200ms for hover, menus and state; scale-from-0.97 plus fade for popups; press feedback by scaling to 0.97; reduced-motion honoured.

## Where they differ

| Axis | One end | Other end |
| --- | --- | --- |
| Neutral temperature | Warm: Claude (`#FAF9F5`, `#141413`, all neutrals yellow-leaning) | Cool: BridgeClip (`#07080C`, blue-tinted inks). T3 and Codex sit at dead neutral (zero chroma). |
| Dark base | Black: T3 `#000`, BridgeClip `#07080C` | Charcoal: Codex `#181818`; Claude warm dark grey [recollection] |
| Contrast | High: T3 dark (`#F5F5F5` on `#000`), BridgeClip (18.9:1) | Softer: Codex dark (white on `#181818`), Claude |
| Radii | Tight and uniform: T3, 8px controls, 4 to 6 inside | Large and capsule: BridgeClip, pill controls, 14 to 18 containers; macOS 26 leans the same way |
| Depth | Flat with hairlines: T3 light, Codex | Layered translucency with rims and deep shadows: BridgeClip, Liquid Glass |
| Accent | Saturated blue: T3 (indigo-blue), Codex (sky blue), BridgeClip (deep blue), macOS | Muted terracotta: Claude |
| Typeface | System face: T3, Codex, macOS | Bundled face: BridgeClip (Geist) |
| Sidebar surface | Tinted a step from the content: T3 light, Codex [recollection], Claude [recollection] | Same as the window, split by a hairline: BridgeClip, T3 dark |

Two things in the references do not transfer directly. Backdrop blur (T3's glass menus, BridgeClip's `glass-thick`, Liquid Glass) needs a blurred copy of what is behind the surface; BridgeClip's own rule is that only floating surfaces blur and panels are flat fills over a solid backdrop, so the flat part of the look survives without it. And three of the five default accents fail 4.5:1 as text or under white text, so REDA's accent needs a separate, darker "accent as text / accent as fill" step in light mode whichever hue is chosen.

## Three directions for the prototype

All three keep the shared traits above and differ on temperature, depth and radius, which is where the references actually disagree. Every text pair listed was checked against WCAG 2 contrast and is at or above 4.5:1 [derived]; the ratios are in the tables. The values are starting points for the prototype, not measurements of any reference. Type sizes are in logical pixels.

### A. Graphite: neutral, flat, hairline (Codex and T3)

Zero-chroma greys, structure from 1px lines, almost no shadow, system-blue accent. The most "native" and the least risky.

| Token | Light | Dark |
| --- | --- | --- |
| Base (window, title bar, Docks) | `#F7F7F8` | `#181818` |
| Raised (panels, inputs, selected tab) | `#FFFFFF` | `#212121` |
| Overlay (menus, popups) | `#FFFFFF` | `#262626` |
| Hover / selected fill | `#ECECEF` / `#E4E4E7` | white 6% / white 10% |
| Border / strong border | `#E4E4E7` / `#D4D4D8` | white 8% / white 12% |
| Text | `#18181B` (16.6:1) | `#EDEDED` (15.2:1) |
| Muted text | `#65656D` (5.4:1; 4.9:1 on hover fill) | `#A1A1A6` (6.9:1; 5.9:1 on overlay) |
| Accent as text / focus ring | `#0A64D6` (5.2:1) | `#4DA3FF` (6.8:1) |
| Accent fill with white text | `#0A64D6` (5.5:1) | `#1F6FE0` (4.8:1) |

- Radii: 4 (menu items, chips), 6 (controls, tabs), 8 (panels, menus), 12 (dialogs).
- Elevation: none on panels. Menus `0 8px 24px -8px` black 18% (45% dark) plus the 1px border. Raised controls get a 1px top highlight in dark.
- Type: Inter, 13px UI in both Densities; 11px captions; 12px Compact row text. Weights 400, 500, 600.
- Control height: 28 Comfortable, 24 Compact. Title bar 40 / 34. Row height 28 / 24.
- Tabs: text-only, selected tab is a Raised fill with 6px radius; no underline, no accent.

### B. Paper: warm, soft, low-contrast (Claude)

Warm off-white and warm charcoal, slightly larger radii, a muted warm accent. The most distinctive; the risk is that warm neutrals fight coloured icons and the canvas, so check it against real content first.

| Token | Light | Dark |
| --- | --- | --- |
| Base (window, title bar, Docks) | `#F0EEE6` | `#1F1E1D` |
| Raised (panels, content, selected tab) | `#FAF9F5` | `#262624` |
| Overlay (menus, popups) | `#FFFFFF` | `#30302E` |
| Hover / selected fill | `#E8E6DC` / `#DEDBCF` | warm white 6% / 10% |
| Border | `#E0DDD2` | warm white 9% |
| Text | `#1F1E1C` (15.8:1 raised, 14.3:1 base) | `#F3F1EA` (13.4:1 on `#262624`) |
| Muted text | `#6B6961` (5.2:1 raised, 4.7:1 base) | `#B0AEA5` (6.8:1; 6.0:1 on overlay) |
| Accent as text / focus ring | `#B04A2F` (5.2:1 raised, 4.7:1 base) | `#E08A6D` (5.8:1; 5.1:1 on overlay) |
| Accent fill | `#B04A2F` with white text (5.4:1) | `#D97757` with `#141413` text (5.9:1) |

- Muted text on the hover fill `#E8E6DC` is below 4.5:1 with these values; rows that show muted text need the text to switch to the primary colour on hover, or a lighter hover fill.
- Radii: 6 (chips, menu items), 8 (controls, tabs), 12 (panels, menus), 16 (dialogs).
- Elevation: panels sit on the Base as Raised cards with the border and `0 1px 2px` black 4%. Menus `0 12px 32px -12px` black 16% (50% dark).
- Type: Inter, 13px UI Comfortable and 12px Compact; 11px captions. A warm theme tolerates slightly looser line height (about 1.4).
- Control height: 30 Comfortable, 26 Compact. Title bar 44 / 36. Row height 30 / 26.
- Tabs: selected tab is a Raised card joined to its panel; unselected tabs are muted text on the Base.

### C. Ink: cool near-black, layered, capsule controls (BridgeMind and T3 dark)

Dark-first. Blue-tinted near-black with translucent white layers, crisp rims, pill controls and a deep blue accent. The most "sleek"; the light theme has to be invented, since the reference has none.

| Token | Light | Dark |
| --- | --- | --- |
| Base (window, title bar, Docks) | `#EEF0F5` | `#07080C` |
| Raised (panels) | `#FFFFFF` | white 4.5% over base (solid fallback `#0E0F15`) |
| Overlay (menus, popups) | `#FFFFFF` | `#1A1C24` |
| Well (inputs) | `#F6F7FA` with inner border | black 26% over the panel |
| Hover / selected fill | `#E3E6EE` / `#D8DCE7` | white 5% / white 11% |
| Border / rim | `#D8DCE6` | white 9%, strong white 16% |
| Text | `#10121A` (16.4:1) | `#F7F8FC` (18.9:1) |
| Muted text | `#5A6072` (5.5:1) | `#AAAEBC` (9.1:1; 7.7:1 on overlay) |
| Accent as text / focus ring | `#1A5FDF` (4.9:1 base, 5.6:1 white) | `#6EA4FF` (8.0:1; 6.8:1 on overlay) |
| Accent fill with white text | `#1A5FDF` (5.6:1) | `#1A5FDF` (5.6:1) |

- Light-mode accent text on the hover fill `#E3E6EE` is 4.49:1, just under the line; darken the accent a step or keep accent-coloured text off hovered rows. Muted text stays above 4.5:1 on both fills (5.0:1 and 4.6:1).
- Radii: full capsule for buttons, chips, segmented controls and tabs; 10 (inputs), 12 (rows, tiles), 14 (panels, menus), 18 (dialogs). Keep nested radii concentric.
- Elevation: every surface has a 1px rim. Panels `0 1px 2px` black 25% plus `0 18px 48px -24px` black 60% in dark, and about a quarter of those opacities in light. Popups `0 40px 100px -24px` black 80% in dark. Raised controls get a 1px white top highlight (14% dark).
- Type: Geist (or Inter) at 13px UI, 11px uppercase semibold group labels with 0.1em tracking, mono with tabular figures for coordinates and measurements.
- Control height: 32 Comfortable, 28 Compact. Title bar 40 / 36. Row height 32 / 26.
- Tabs: a segmented capsule track with the selected tab as a raised pill.

### Shared by all three

- Accent appears on: the primary button, the focus ring, switch-on and checked states, the active Tool, the selection box on the canvas, progress. Hover and list selection are neutral fills. Destructive is red, and status hues appear only for status.
- Focus ring: 2px accent at about 75% with a 2px offset.
- Motion: 150ms for hover and state, 140ms menus (fade plus scale from 0.97), 200ms for Dock collapse, easing `cubic-bezier(0.16, 1, 0.3, 1)`; press scales to 0.97; all of it off under reduced motion.
- Title bar: window colour, no bottom border, sidebar or Dock toggles at the leading edge, title or breadcrumb, pane toggles at the trailing edge.

The sharpest comparison on one screen is A against C in dark (flat charcoal against layered near-black) and A against B in light (cool neutral against warm paper). If only two directions are built, A and C differ on the most axes.

## Sources

- T3 Code: [`pingdotgg/t3code` @ `dd50613`](https://github.com/pingdotgg/t3code/tree/dd5061364bbf52f5008a16fb22e367c3680bdbd1): `apps/web/src/index.css`, `apps/web/components.json`, `apps/web/src/components/ui/{button,input,sidebar,menu,dialog,tooltip,toggle,switch}.tsx`, `apps/web/src/appearanceFonts.ts`; official screenshot `app-desktop` on [t3.codes](https://t3.codes/).
- BridgeMind: [`bridge-mind/bridgeclip` @ `b996820`](https://github.com/bridge-mind/bridgeclip/tree/b996820ef80fa324a3ad233bef1aac5cc6936804): `DESIGN.md`, `src/renderer/globals.css`, `tailwind.config.ts`, `resources/social/bridgeclip-app-demo.png`. `bridgemind.ai` and `docs.bridgemind.ai` were unreachable (HTTP 403).
- Codex: [Settings reference](https://developers.openai.com/codex/app/settings), [app overview](https://developers.openai.com/codex/app), [changelog](https://developers.openai.com/codex/changelog). The public [`openai/codex`](https://github.com/openai/codex) repo contains no desktop UI code.
- Claude: [Desktop application docs](https://code.claude.com/docs/en/desktop), [Redesigning Claude Code on desktop for parallel agents](https://claude.com/blog/claude-code-desktop-redesign), [`anthropics/skills` brand-guidelines](https://github.com/anthropics/skills/blob/main/skills/brand-guidelines/SKILL.md).
- Apple: Human Interface Guidelines pages for [Typography](https://developer.apple.com/design/human-interface-guidelines/typography), [Color](https://developer.apple.com/design/human-interface-guidelines/color), [Materials](https://developer.apple.com/design/human-interface-guidelines/materials), [Toolbars](https://developer.apple.com/design/human-interface-guidelines/toolbars), [Sidebars](https://developer.apple.com/design/human-interface-guidelines/sidebars), [Buttons](https://developer.apple.com/design/human-interface-guidelines/buttons), [Windows](https://developer.apple.com/design/human-interface-guidelines/windows), [Motion](https://developer.apple.com/design/human-interface-guidelines/motion).
