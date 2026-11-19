//! Tool Command ids at the boundary between the public registry and the window.

/// A typed enum of view Command ids the window acts on after they run, with `id` and
/// `from_id`, so the Shell parses an id once and dispatches on the variant.
macro_rules! typed_commands {
    ($(#[$meta:meta])* $name:ident { $($command:ident => $id:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub(crate) enum $name {
            $($command),+
        }

        impl $name {
            /// The public id used by the Command's metadata.
            pub(crate) const fn id(self) -> &'static str {
                match self {
                    $(Self::$command => $id,)+
                }
            }

            /// Resolves a base Command id after the caller has separated any Preset.
            pub(crate) fn from_id(id: &str) -> Option<Self> {
                match id {
                    $($id => Some(Self::$command),)+
                    _ => None,
                }
            }
        }
    };
}
pub(crate) use typed_commands;

typed_commands! {
    /// The `tool.*` Commands.
    ToolCommand {
    Activate => "tool.activate",
    RotateAbout => "tool.rotate_about",
    Rotate => "tool.rotate",
    MirrorX => "tool.mirror_x",
    MirrorY => "tool.mirror_y",
    FlipSide => "tool.flip_side",
    MoveBoard => "tool.move_board",
    MoveBoardShape => "tool.move_board_shape",
    CycleCorner => "tool.cycle_corner",
    UndoSegment => "tool.undo_segment",
    Finish => "tool.finish",
    CycleWireMode => "tool.cycle_wire_mode",
    CycleCuttingLength => "tool.cycle_cutting_length",
    InsertVertex => "tool.insert_vertex",
    EditProperties => "tool.edit_properties",
    SetAutoWire => "tool.auto_wire.set",
    Copy => "tool.copy",
    Duplicate => "tool.duplicate",
    Paste => "tool.paste",
    Place => "tool.place",
    SetPlacement => "tool.place.set",
    PlaceUnit => "tool.place_unit",
    }
}

#[cfg(test)]
mod tests {
    use super::ToolCommand;
    use crate::shell::Shell;
    use reda_engine::Engine;

    #[test]
    fn every_registered_tool_command_has_a_typed_action() {
        let shell = Shell::new(Engine::builder().headless(false).user_dirs(None).build());
        for meta in shell
            .engine()
            .registry()
            .commands()
            .filter(|m| m.id.starts_with("tool."))
        {
            assert_eq!(
                ToolCommand::from_id(meta.id).map(ToolCommand::id),
                Some(meta.id)
            );
        }
    }
}
