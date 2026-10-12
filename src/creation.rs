//! Empty-canvas double clicks complete on mouse release, like master
//! ControllerEntityCreate. Reuse the existing hit tests and document history.
use super::*;
use std::time::Duration;

// Logical screen pixels: tolerate touchpad tap drift without accepting a drag.
pub(super) const CLICK_SLOP_SQUARED: f32 = 8.0 * 8.0;
pub(super) const DOUBLE_CLICK_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Default)]
pub struct Gesture {
    pressed: Option<(Vec2, Vec3)>,
    previous: Option<(Instant, Vec2, Vec3)>,
}

impl Gesture {
    pub fn cancel(&mut self) {
        *self = default();
    }

    pub fn press(&mut self, cursor: Vec2, view: Vec3) {
        self.pressed = Some((cursor, view));
    }

    pub fn observe(&mut self, cursor: Vec2, view: Vec3) {
        if self.pressed.is_some_and(|(start, camera)| {
            start.distance_squared(cursor) > CLICK_SLOP_SQUARED || camera != view
        }) || self.previous.is_some_and(|(_, _, camera)| camera != view)
        {
            self.cancel();
        }
    }

    pub fn release(&mut self, cursor: Vec2, view: Vec3, now: Instant, empty: bool) -> bool {
        self.observe(cursor, view);
        if self.pressed.take().is_none() || !empty {
            self.cancel();
            return false;
        }
        if self
            .previous
            .take()
            .is_some_and(|(time, position, camera)| {
                now.saturating_duration_since(time) <= DOUBLE_CLICK_INTERVAL
                    && cursor.distance_squared(position) < 400.0
                    && camera == view
            })
        {
            true
        } else {
            self.previous = Some((now, cursor, view));
            false
        }
    }
}

pub fn create(
    cursor: Vec2,
    doc: &mut Document,
    interaction: &mut Interaction,
    editor: &mut editor::Editor,
    physics: &mut physics::PhysicsEditor,
    effects: &mut effects::Effects,
) {
    doc.selection = project_graph_bevy_canvas::document::Selection::capture(
        doc,
        &interaction.selected,
        &interaction.selected_edges,
    );
    physics.finish(doc);
    match doc.create_text_node([cursor.x, -cursor.y]) {
        Ok(index) => {
            let node = &doc.nodes[index];
            let size = Vec2::new(node.rect[2], node.rect[3]);
            effects.create_node(center(node), size);
            interaction.selected = HashSet::from([index]);
            interaction.selected_edges.clear();
            interaction.selection_revision = doc.revision;
            editor.begin_new(node);
            interaction.last_click = None;
            interaction.dirty = true;
        }
        Err(error) => editor.report_error(error.to_string()),
    }
}

#[cfg(test)]
#[path = "creation_tests.rs"]
mod tests;
