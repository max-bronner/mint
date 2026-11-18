use crate::geom::{ColorTransform, Matrix};
use crate::instance::{InstanceId, InstanceKind};
use crate::library::CharacterRef;
use crate::player::Player;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RenderCommand {
    Shape {
        instance: InstanceId,
        character: CharacterRef,
        transform: Matrix,
        color: ColorTransform,
    },
    Morph {
        instance: InstanceId,
        character: CharacterRef,
        ratio: f32,
        transform: Matrix,
        color: ColorTransform,
    },
    EditText {
        instance: InstanceId,
        character: CharacterRef,
        transform: Matrix,
        color: ColorTransform,
    },
    StaticText {
        instance: InstanceId,
        character: CharacterRef,
        transform: Matrix,
        color: ColorTransform,
    },
    Surface {
        instance: InstanceId,
        bounds: [f32; 4],
        transform: Matrix,
        color: ColorTransform,
    },
    PushMask,
    ActivateMask,
    DeactivateMask,
    PopMask,
}

#[derive(Debug, Default)]
pub struct RenderList {
    pub commands: Vec<RenderCommand>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderOptions {
    pub surfaces: bool,
}

impl Player {
    pub fn render(&self, out: &mut RenderList) {
        self.render_with(out, RenderOptions::default());
    }

    pub fn render_with(&self, out: &mut RenderList, options: RenderOptions) {
        out.commands.clear();
        self.render_instance(
            self.root(),
            &Matrix::IDENTITY,
            &ColorTransform::IDENTITY,
            false,
            options,
            out,
        );
    }

    fn is_surface(&self, id: InstanceId) -> bool {
        self.own_string(id, "_type").is_some_and(|s| !s.is_empty())
    }

    #[must_use]
    pub fn own_string(&self, id: InstanceId, name: &str) -> Option<std::sync::Arc<str>> {
        let stage = &self.stage;
        match stage
            .arena
            .get(id)
            .and_then(|i| i.script)
            .and_then(|o| stage.heap.objects.get(o))
            .and_then(|o| o.props.get(name))
        {
            Some(crate::avm::Value::String(s)) => Some(std::sync::Arc::clone(s)),
            _ => None,
        }
    }

    fn render_instance(
        &self,
        id: InstanceId,
        parent_m: &Matrix,
        parent_c: &ColorTransform,
        drawing_mask: bool,
        options: RenderOptions,
        out: &mut RenderList,
    ) {
        let Some(inst) = self.instance(id) else {
            return;
        };
        let transform = parent_m.concat(&inst.matrix);
        let color = parent_c.concat(&inst.color);
        let character = inst.character;
        if options.surfaces
            && !drawing_mask
            && matches!(inst.kind, InstanceKind::Clip(_))
            && self.is_surface(id)
        {
            if let Some(bounds) = self.local_bounds(id) {
                out.commands.push(RenderCommand::Surface {
                    instance: id,
                    bounds,
                    transform,
                    color,
                });
            }
            return;
        }
        let command = match &inst.kind {
            InstanceKind::Shape => RenderCommand::Shape {
                instance: id,
                character,
                transform,
                color,
            },
            InstanceKind::Morph => RenderCommand::Morph {
                instance: id,
                character,
                ratio: inst.ratio,
                transform,
                color,
            },
            InstanceKind::EditText { .. } => RenderCommand::EditText {
                instance: id,
                character,
                transform,
                color,
            },
            InstanceKind::StaticText => RenderCommand::StaticText {
                instance: id,
                character,
                transform,
                color,
            },
            InstanceKind::Clip(_) | InstanceKind::Button(_) => {
                self.render_children(id, &transform, &color, drawing_mask, options, out);
                return;
            }
        };
        out.commands.push(command);
    }

    fn render_children(
        &self,
        id: InstanceId,
        m: &Matrix,
        c: &ColorTransform,
        drawing_mask: bool,
        options: RenderOptions,
        out: &mut RenderList,
    ) {
        let Some(inst) = self.instance(id) else {
            return;
        };
        let mut clip_depth = 0i32;
        let mut stack: Vec<(i32, std::ops::Range<usize>)> = Vec::new();
        for (depth, child_id) in inst.children() {
            let Some(child) = self.instance(child_id) else {
                continue;
            };
            while clip_depth > 0 && depth > clip_depth {
                let Some((previous, range)) = stack.pop() else {
                    break;
                };
                clip_depth = previous;
                out.commands.push(RenderCommand::DeactivateMask);
                out.commands.extend_from_within(range);
                out.commands.push(RenderCommand::PopMask);
            }
            let allow_as_mask = match child.kind {
                InstanceKind::Clip(_) => !child.children.is_empty(),
                _ => true,
            };
            if child.clip_depth > 0 && allow_as_mask {
                out.commands.push(RenderCommand::PushMask);
                let start = out.commands.len();
                self.render_instance(child_id, m, c, true, options, out);
                let end = out.commands.len();
                out.commands.push(RenderCommand::ActivateMask);
                stack.push((clip_depth, start..end));
                clip_depth = child.clip_depth;
            } else if child.visible || drawing_mask {
                self.render_instance(child_id, m, c, drawing_mask, options, out);
            }
        }
        while let Some((_, range)) = stack.pop() {
            out.commands.push(RenderCommand::DeactivateMask);
            out.commands.extend_from_within(range);
            out.commands.push(RenderCommand::PopMask);
        }
    }
}
