//! Creation feedback, separate from cutting feedback for independent changes.
use super::*;

impl Effects {
    pub fn create_node(&mut self, center: Vec2, size: Vec2) {
        if valid_rect(center, size) {
            let palette = super::super::theme::palette(self.light);
            self.push(
                12.0,
                Shape::CreateFlash {
                    center,
                    size,
                    color: palette.accent,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_feedback_stays_near_node_at_every_zoom_and_finishes() {
        let mut effects = Effects::default();
        effects.create_node(Vec2::ZERO, Vec2::new(160.0, 80.0));
        assert_eq!(effects.items.len(), 1);
        assert!(
            effects
                .items
                .iter()
                .all(|e| matches!(e.shape, Shape::CreateFlash { .. }))
        );
        for scale in [0.1, 1.0, 10.0] {
            let mesh = geometry(&effects, scale).unwrap();
            let Some(bevy::mesh::VertexAttributeValues::Float32x3(vertices)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("missing positions")
            };
            assert!(!vertices.is_empty());
            for point in vertices {
                assert!(point[0].abs() <= 80.0 + 2.0 * scale);
                assert!(point[1].abs() <= 40.0 + 2.0 * scale);
            }
        }
        effects.advance(0.21);
        assert!(!effects.active());
    }
}
