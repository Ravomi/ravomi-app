use super::*;
use avian2d::collider_tree::{
    ColliderTreeProxyKey, ColliderTreeSystems, ColliderTrees, MovedProxies,
};
use avian2d::collision::broad_phase::BroadPhaseSystems;
use avian2d::collision::collider::EnlargedAabb;
use avian2d::physics_transform::PhysicsTransformConfig;
use avian2d::prelude::*;

const NORMAL: u32 = 1;
const MOVING: u32 = 2;
const LOCKED: u32 = 4;

#[derive(Component)]
pub(super) struct BodyNode {
    index: usize,
    id: String,
    size: Vec2,
    members: Vec<usize>,
    locked: bool,
}

#[derive(Resource, Default)]
pub struct PhysicsEditor {
    pub enabled: bool,
    bodies_spawned: bool,
    node_count: usize,
    pub running: bool,
    pub dragging: bool,
    pub target: Vec2,
    pub drag_scale: f32,
    offset: Vec2,
    selection: HashSet<usize>,
    passive: HashMap<usize, Vec2>,
    reset: bool,
    revision: u64,
    stable: f32,
    elapsed: f32,
    pub step_ms: f64,
    pub active: usize,
    pub sleeping: usize,
    pub measure: bool,
    pub samples: Vec<f64>,
    pub candidate_pairs: usize,
    pub contacts: u32,
    pub display: Vec<Option<Vec2>>,
}
impl PhysicsEditor {
    pub fn finish(&mut self, doc: &mut Document) {
        doc.finish();
        self.pause_drag();
    }
    // Hand an ongoing drag to the Alt placement tool without splitting history.
    pub fn pause_drag(&mut self) {
        self.running = false;
        self.dragging = false;
        self.reset = true;
        self.selection.clear();
        self.passive.clear();
        self.display.clear();
    }
    pub fn cancel(&mut self, doc: &mut Document) {
        doc.cancel();
        self.finish(doc);
    }
    pub fn toggle(&mut self, doc: &mut Document) {
        self.finish(doc);
        self.enabled = !self.enabled;
    }
    pub fn begin(&mut self, doc: &mut Document, selected: &HashSet<usize>) {
        self.finish(doc);
        self.selection = doc.expand_selection(selected);
        let protected = doc.protected_nodes();
        self.selection.retain(|i| !protected.contains(i));
        if self.selection.is_empty() {
            return;
        }
        doc.begin("移动并推开节点");
        let covered: HashSet<_> = collision_units(doc, &self.selection)
            .into_values()
            .flatten()
            .collect();
        self.passive = self
            .selection
            .iter()
            .filter_map(|&i| {
                let n = &doc.nodes[i];
                (!covered.contains(&i) && n.kind == "ConnectPoint").then_some((i, center(n)))
            })
            .collect();
        self.target = Vec2::ZERO;
        self.offset = Vec2::ZERO;
        self.running = true;
        self.dragging = true;
        self.stable = 0.0;
        self.elapsed = 0.0;
    }
    pub fn release(&mut self) {
        self.dragging = false;
        self.stable = 0.0;
        self.elapsed = 0.0;
    }
}
pub fn enabled_from_args(args: &[String]) -> bool {
    !args.iter().any(|arg| arg == "--no-physics")
        && (!args.iter().any(|arg| arg == "--benchmark")
            || args.iter().any(|arg| arg == "--physics"))
}
fn collision_units(doc: &Document, selected: &HashSet<usize>) -> HashMap<usize, Vec<usize>> {
    let indices: HashMap<_, _> = doc
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let mut groups = Vec::new();
    for (i, node) in doc
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.kind == "Section")
    {
        let mut members = HashSet::from([i]);
        let mut pending: Vec<_> = node
            .children
            .iter()
            .filter_map(|id| indices.get(id.as_str()).copied())
            .collect();
        while let Some(child) = pending.pop() {
            if members.insert(child) {
                pending.extend(
                    doc.nodes[child]
                        .children
                        .iter()
                        .filter_map(|id| indices.get(id.as_str()).copied()),
                );
            }
        }
        // Editing a member opens its ancestors for internal movement; an intact
        // group has one collider so its children cannot collide with their frame.
        if members.iter().any(|i| selected.contains(i))
            && !members.iter().all(|i| selected.contains(i))
        {
            continue;
        }
        let mut members: Vec<_> = members.into_iter().collect();
        members.sort_unstable();
        groups.push((i, members));
    }
    groups.sort_by_key(|(i, members)| (std::cmp::Reverse(members.len()), *i));
    let mut covered = HashSet::new();
    let mut units = HashMap::new();
    for (i, members) in groups {
        if members.iter().any(|i| covered.contains(i)) {
            continue;
        }
        covered.extend(members.iter().copied());
        units.insert(i, members);
    }
    for (i, node) in doc.nodes.iter().enumerate() {
        if !covered.contains(&i) && node.kind != "Section" && node.kind != "ConnectPoint" {
            units.insert(i, vec![i]);
        }
    }
    units
}

pub fn install(app: &mut App) {
    // Bodies are independent roots driven by Position/velocity; collider sizes
    // are synchronized explicitly. Run after this frame's Egui canvas input so
    // pointer motion reaches both the solver and rendering in the same frame.
    app.insert_resource(PhysicsTransformConfig {
        propagate_before_physics: false,
        transform_to_position: false,
        transform_to_collider_scale: false,
        position_to_transform: true,
    })
    .add_plugins(PhysicsPlugins::new(PostUpdate).with_length_unit(100.0))
    .add_plugins(avian2d::diagnostics::PhysicsTotalDiagnosticsPlugin)
    .insert_resource(Gravity(Vec2::ZERO))
    .init_resource::<PhysicsEditor>()
    .add_systems(Startup, setup.after(super::setup))
    .add_systems(
        PostUpdate,
        synchronize
            .after(bevy_egui::EguiPostUpdateSet::EndPass)
            .before(PhysicsSystems::First),
    )
    .add_systems(
        PostUpdate,
        drive
            .after(PhysicsSystems::Prepare)
            .before(PhysicsSystems::StepSimulation),
    )
    .add_systems(
        PostUpdate,
        (readback, synchronize_display)
            .chain()
            .after(PhysicsSystems::Last)
            .before(super::sync_nodes),
    )
    .add_systems(
        PhysicsSchedule,
        refresh_drag_bounds
            .after(ColliderTreeSystems::UpdateAabbs)
            .before(BroadPhaseSystems::CollectCollisions),
    );
}
fn setup(
    mut commands: Commands,
    mut scene: ResMut<Scene>,
    mut clock: ResMut<Time<Physics>>,
    config: Option<Res<Config>>,
    mut state: ResMut<PhysicsEditor>,
) {
    clock.pause();
    if let Some(config) = config {
        state.measure = config.benchmark;
        if config.physics {
            state.toggle(&mut scene.0);
        }
    }
    if state.enabled {
        spawn_bodies(&mut commands, &scene.0);
        state.bodies_spawned = true;
        state.node_count = scene.0.nodes.len();
    }
}
fn spawn_bodies(commands: &mut Commands, doc: &Document) {
    let protected = doc.protected_nodes();
    for (index, members) in collision_units(doc, &HashSet::new()) {
        let locked = members.iter().any(|i| protected.contains(i));
        spawn_body(commands, index, &doc.nodes[index], members, locked);
    }
}
fn spawn_body(
    commands: &mut Commands,
    index: usize,
    node: &GraphNode,
    members: Vec<usize>,
    locked: bool,
) {
    commands.spawn((
        BodyNode {
            index,
            id: node.id.clone(),
            size: Vec2::new(node.rect[2], node.rect[3]),
            members,
            locked,
        },
        if locked {
            RigidBody::Static
        } else {
            RigidBody::Dynamic
        },
        Collider::rectangle(node.rect[2], node.rect[3]),
        Position(center(node)),
        Rotation::default(),
        Transform::from_translation(center(node).extend(0.0)),
        LockedAxes::ROTATION_LOCKED,
        LinearDamping(16.0),
        Friction::ZERO,
        Restitution::ZERO,
        CollisionLayers::new(if locked { LOCKED } else { NORMAL }, u32::MAX),
    ));
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn synchronize(
    mut commands: Commands,
    mut scene: ResMut<Scene>,
    display: Option<Res<sections::Display>>,
    mut state: ResMut<PhysicsEditor>,
    mut clock: ResMut<Time<Physics>>,
    mut bodies: Query<(
        Entity,
        &mut BodyNode,
        &mut Position,
        &mut Transform,
        &mut LinearVelocity,
        &RigidBody,
        &CollisionLayers,
        &mut Collider,
    )>,
) {
    if state.enabled != state.bodies_spawned {
        clock.pause();
        clock.advance_by(std::time::Duration::ZERO);
        if state.enabled {
            spawn_bodies(&mut commands, &scene.0);
        } else {
            for (entity, ..) in &bodies {
                commands.entity(entity).despawn();
            }
            state.active = 0;
            state.sleeping = 0;
            state.candidate_pairs = 0;
            state.contacts = 0;
        }
        state.bodies_spawned = state.enabled;
        state.node_count = scene.0.nodes.len();
        state.reset = true;
        return;
    }
    if state.enabled
        && (state.reset
            || state.revision != scene.0.revision
            || state.node_count != scene.0.nodes.len())
    {
        let stale = bodies.iter().any(|(_, body, ..)| {
            scene
                .0
                .nodes
                .get(body.index)
                .is_none_or(|node| node.id != body.id)
        });
        if stale {
            state.finish(&mut scene.0);
        }
        let units = collision_units(&scene.0, &state.selection);
        let protected = scene.0.protected_nodes();
        let mut present = HashSet::new();
        let mut removed = false;
        for (entity, mut body, ..) in &mut bodies {
            if scene
                .0
                .nodes
                .get(body.index)
                .is_some_and(|node| node.id == body.id && units.contains_key(&body.index))
            {
                let index = body.index;
                body.members.clone_from(&units[&index]);
                body.locked = body.members.iter().any(|i| protected.contains(i));
                present.insert(body.index);
            } else {
                commands.entity(entity).despawn();
                removed = true;
            }
        }
        let mut added = false;
        for (index, members) in units.into_iter().filter(|(i, _)| !present.contains(i)) {
            let locked = members.iter().any(|i| protected.contains(i));
            spawn_body(&mut commands, index, &scene.0.nodes[index], members, locked);
            added = true;
        }
        state.node_count = scene.0.nodes.len();
        if removed || added {
            clock.pause();
            clock.advance_by(std::time::Duration::ZERO);
            state.reset = true;
            state.display.clear();
            return;
        }
    }
    // Collapsed sections retain their expanded document bounds. Use the same
    // projected geometry as rendering, including measurements arriving later.
    for (_, mut body, .., mut collider) in &mut bodies {
        let graph = display.as_ref().map_or(&scene.0.graph, |d| d.graph(&scene));
        let Some(node) = graph.nodes.get(body.index).filter(|n| n.id == body.id) else {
            continue;
        };
        let size = Vec2::new(node.rect[2], node.rect[3]);
        if body.size != size {
            *collider = Collider::rectangle(size.x, size.y);
            body.size = size;
        }
    }
    if state.reset || state.revision != scene.0.revision {
        for (entity, body, mut pos, mut transform, mut velocity, rigid, layers, _) in &mut bodies {
            let Some(n) = scene.0.nodes.get(body.index).filter(|n| n.id == body.id) else {
                continue;
            };
            pos.set_if_neq(Position(center(n)));
            if transform.translation != pos.0.extend(0.0) {
                transform.translation = pos.0.extend(0.0);
            }
            velocity.set_if_neq(LinearVelocity::ZERO);
            let next_rigid = if body.locked {
                RigidBody::Static
            } else if state.selection.contains(&body.index) {
                RigidBody::Kinematic
            } else {
                RigidBody::Dynamic
            };
            if *rigid != next_rigid {
                commands.entity(entity).insert(next_rigid);
            }
            let next_layers = CollisionLayers::new(
                if body.locked {
                    LOCKED
                } else if state.selection.contains(&body.index) {
                    MOVING
                } else {
                    NORMAL
                },
                if state.selection.contains(&body.index) {
                    NORMAL | LOCKED
                } else {
                    u32::MAX
                },
            );
            if *layers != next_layers {
                commands.entity(entity).insert(next_layers);
            }
            if !state.running {
                if !body.locked {
                    commands.entity(entity).insert(Sleeping);
                }
            } else if !state.dragging || state.selection.contains(&body.index) {
                commands.entity(entity).remove::<Sleeping>();
            }
            // Continuous casts in drive() clip the entire selection. Per-body
            // swept CCD could shorten individual members and break formation.
        }
        state.revision = scene.0.revision;
        state.reset = false;
        if !state.running {
            state.active = 0;
            state.sleeping = if state.enabled {
                bodies.iter().filter(|(_, body, ..)| !body.locked).count()
            } else {
                0
            };
        }
    }
    if state.enabled && state.running {
        clock.unpause();
    } else {
        clock.pause();
        clock.advance_by(std::time::Duration::ZERO);
    }
}

fn synchronize_display(
    scene: Res<Scene>,
    mut state: ResMut<PhysicsEditor>,
    bodies: Query<(&BodyNode, &Position)>,
    mut interaction: ResMut<Interaction>,
) {
    if state.running {
        state.display.resize(scene.0.nodes.len(), None);
        let mut adjustment = Vec2::ZERO;
        for (body, position) in &bodies {
            let Some(node) = scene
                .0
                .nodes
                .get(body.index)
                .filter(|node| node.id == body.id)
            else {
                continue;
            };
            let p = position.0;
            let delta = p - center(node);
            for &i in &body.members {
                state.display[i] = Some(center(&scene.0.nodes[i]) + delta);
            }
            if state.selection.contains(&body.index) {
                adjustment = p - center(node);
            }
        }
        let passive: Vec<_> = state.passive.keys().copied().collect();
        for i in passive {
            if let Some(node) = scene.0.nodes.get(i) {
                state.display[i] = Some(center(node) + adjustment);
            }
        }
        interaction.dirty = true;
    }
}

pub fn visual_center(state: &PhysicsEditor, index: usize, node: &GraphNode) -> Vec2 {
    state
        .display
        .get(index)
        .copied()
        .flatten()
        .unwrap_or_else(|| center(node))
}

#[allow(clippy::type_complexity)]
fn drive(
    mut state: ResMut<PhysicsEditor>,
    time: Res<Time>,
    spatial: SpatialQuery,
    mut bodies: Query<(&BodyNode, &Position, &Collider, &mut LinearVelocity)>,
) {
    if !state.enabled || !state.running || state.reset || time.delta().is_zero() {
        return;
    }
    let scale = if state.drag_scale > 0.0 && state.drag_scale.is_finite() {
        state.drag_scale
    } else {
        1.0
    };
    // The pointer is an exact placement target, not a spring destination. The
    // continuous cast still clips the whole formation against locked obstacles.
    let mut delta = state.target - state.offset;
    if let Ok(direction) = Dir2::new(delta) {
        let mut distance = delta.length();
        for (node, position, collider, _) in &bodies {
            if !state.selection.contains(&node.index) {
                continue;
            }
            let cast = ShapeCastConfig {
                ignore_origin_penetration: true,
                ..ShapeCastConfig::from_max_distance(distance)
            };
            if let Some(hit) = spatial.cast_shape(
                collider,
                position.0,
                0.0,
                direction,
                &cast,
                &SpatialQueryFilter::from_mask(LOCKED),
            ) {
                distance = distance.min((hit.distance - 0.1 * scale).max(0.0));
            }
        }
        delta = direction.as_vec2() * distance;
    }
    for (node, _, _, mut velocity) in &mut bodies {
        if state.selection.contains(&node.index) {
            velocity.0 = delta / time.delta_secs();
        }
    }
    state.offset += delta;
}

#[allow(clippy::type_complexity)]
fn refresh_drag_bounds(
    state: Res<PhysicsEditor>,
    time: Res<Time>,
    mut trees: ResMut<ColliderTrees>,
    mut moved: ResMut<MovedProxies>,
    mut bodies: Query<(
        &BodyNode,
        &Position,
        &Collider,
        &LinearVelocity,
        &mut EnlargedAabb,
        &ColliderTreeProxyKey,
    )>,
) {
    // Avian 0.7 predicts swept bounds at the end of the preceding step. A new
    // pointer velocity needs bounds before this step's broad phase as well;
    // otherwise a fast move can skip a neighbor before it is ever detected.
    for (node, position, collider, velocity, mut bounds, proxy) in &mut bodies {
        if !state.selection.contains(&node.index) || velocity.0 == Vec2::ZERO {
            continue;
        }
        let swept = collider.swept_aabb(
            position.0,
            0.0,
            position.0 + velocity.0 * time.delta_secs(),
            0.0,
        );
        if bounds.update(&swept, 0.1) {
            trees
                .tree_for_type_mut(proxy.tree_type())
                .resize_proxy_aabb(proxy.id(), bounds.get().into());
            moved.insert(*proxy);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn readback(
    mut state: ResMut<PhysicsEditor>,
    mut scene: ResMut<Scene>,
    mut interaction: ResMut<Interaction>,
    time: Res<Time>,
    bodies: Query<(&BodyNode, &Position, &LinearVelocity, Has<Sleeping>)>,
    mut clock: ResMut<Time<Physics>>,
    diagnostics: Res<avian2d::diagnostics::PhysicsTotalDiagnostics>,
    collisions: Res<avian2d::collision::CollisionDiagnostics>,
    contacts: Res<ContactGraph>,
) {
    if !state.enabled || !state.running || state.reset {
        return;
    }
    state.step_ms = diagnostics.step_time.as_secs_f64() * 1000.0;
    if state.measure {
        let ms = state.step_ms;
        state.samples.push(ms);
        state.candidate_pairs = contacts.iter_active().count() + contacts.iter_sleeping().count();
        state.contacts = collisions.contact_count;
    }
    state.active = 0;
    state.sleeping = 0;
    let mut fast = false;
    for (body, position, velocity, asleep) in &bodies {
        let Some(node) = scene
            .0
            .nodes
            .get(body.index)
            .filter(|node| node.id == body.id)
        else {
            continue;
        };
        if body.locked {
            continue;
        }
        if asleep {
            state.sleeping += 1;
        } else {
            state.active += 1;
        }
        let delta = position.0 - center(node);
        if delta.length_squared() > 0.000001 {
            for &i in &body.members {
                let node = &mut scene.0.nodes[i];
                node.rect[0] += delta.x;
                node.rect[1] -= delta.y;
            }
            interaction.dirty = true;
        }
        fast |= velocity.length_squared() > 1.0;
    }
    for (&i, &origin) in &state.passive {
        let Some(node) = scene.0.nodes.get_mut(i) else {
            continue;
        };
        let p = origin + state.offset;
        node.rect[0] = p.x - node.rect[2] * 0.5;
        node.rect[1] = -p.y - node.rect[3] * 0.5;
        interaction.dirty = true;
    }
    if !state.dragging {
        state.elapsed += time.delta_secs();
        state.stable = if fast {
            0.0
        } else {
            state.stable + time.delta_secs()
        };
        if state.stable > 0.2 || state.elapsed > 2.0 {
            state.finish(&mut scene.0);
            clock.pause();
            clock.advance_by(std::time::Duration::ZERO);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use project_graph_bevy_canvas::GraphEdge;
    fn app() -> App {
        app_with_config(None)
    }
    fn app_with_config(config: Option<Config>) -> App {
        let node = |id: &str, x: f32, y: f32, locked| GraphNode {
            id: id.into(),
            kind: "TextNode".into(),
            rect: [x, y, 40.0, 40.0],
            text: id.into(),
            url: None,
            font_size: 20.0,
            auto_size: false,
            color: [0.0; 4],
            attachment: None,
            source_name: None,
            children: vec![],
            locked,
            collapsed: false,
            stroke: vec![],
        };
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_secs_f64(1.0 / 120.0),
            ))
            .insert_resource(Scene(Document::memory(Graph {
                nodes: vec![
                    node("a", 0.0, 0.0, false),
                    node("b", 55.0, 0.0, false),
                    node("c", 0.0, 80.0, false),
                    node("wall", 200.0, -200.0, true),
                ],
                edges: vec![],
                attachments: Default::default(),
                counts: Default::default(),
            })))
            .init_resource::<Interaction>();
        // A tall fixed obstacle blocks both selected nodes.
        app.world_mut().resource_mut::<Scene>().0.nodes[3].rect[3] = 500.0;
        if let Some(config) = config {
            app.world_mut().resource_mut::<Scene>().0.nodes[1].rect[0] = 10.0;
            app.insert_resource(config);
        }
        install(&mut app);
        app.finish();
        app.cleanup();
        app.update();
        app
    }
    #[test]
    fn interaction_defaults_to_push_but_benchmark_requires_an_explicit_flag() {
        for (args, enabled) in [
            (vec![], true),
            (vec!["--physics"], true),
            (vec!["--no-physics"], false),
            (vec!["--benchmark"], false),
            (vec!["--benchmark", "--physics"], true),
            (vec!["--physics", "--no-physics"], false),
            (vec!["--benchmark", "--physics", "--no-physics"], false),
        ] {
            let args: Vec<String> = args.into_iter().map(str::to_owned).collect();
            assert_eq!(enabled_from_args(&args), enabled, "{args:?}");
        }
    }
    #[test]
    fn loading_and_toggling_push_preserve_overlapping_nodes_without_history() {
        let mut app = app_with_config(Some(Config {
            font: vec![],
            benchmark: false,
            output: "unused".into(),
            screenshot: None,
            light: false,
            ui_scale: 1.0,
            physics: enabled_from_args(&[]),
            no_ui: false,
            preview: false,
            edit_node: None,
            diagnostics: false,
        }));
        let before = app.world().resource::<Scene>().0.nodes.clone();
        assert_eq!(before[1].rect[0], 10.0);
        assert!(app.world().resource::<PhysicsEditor>().enabled);
        for _ in 0..30 {
            app.update();
        }
        for _ in 0..2 {
            app.world_mut()
                .resource_scope(|world, mut state: Mut<PhysicsEditor>| {
                    state.toggle(&mut world.resource_mut::<Scene>().0);
                    assert!(!state.running && !state.dragging);
                });
            for _ in 0..30 {
                app.update();
            }
            let doc = &app.world().resource::<Scene>().0;
            assert_eq!(doc.nodes, before);
            assert!(!doc.dirty());
            assert!(!doc.history.can_undo());
            assert!(app.world().resource::<Time<Physics>>().is_paused());
        }
    }
    #[test]
    fn enabled_bodies_follow_group_undo_and_node_addition_removal_without_stale_indices() {
        let mut app = app();
        app.world_mut()
            .resource_scope(|world, mut state: Mut<PhysicsEditor>| {
                state.toggle(&mut world.resource_mut::<Scene>().0);
            });
        app.update();
        app.update();
        app.world_mut()
            .resource_mut::<Scene>()
            .0
            .create_group(&HashSet::from([0, 1]))
            .unwrap();
        app.update();
        assert_eq!(
            app.world_mut()
                .query::<&BodyNode>()
                .iter(app.world())
                .count(),
            3
        );
        app.world_mut().resource_mut::<Scene>().0.undo();
        app.update();
        let mut node = app.world().resource::<Scene>().0.nodes[0].clone();
        node.id = "added".into();
        node.rect[0] = 1000.0;
        app.world_mut().resource_mut::<Scene>().0.nodes.push(node);
        app.update();
        app.update();
        assert_eq!(
            app.world_mut()
                .query::<&BodyNode>()
                .iter(app.world())
                .count(),
            5
        );
        app.world_mut().resource_mut::<Scene>().0.nodes.remove(0);
        app.update();
        app.update();
        let mut bodies = app.world_mut().query::<&BodyNode>();
        let graph = &app.world().resource::<Scene>().0;
        for body in bodies.iter(app.world()) {
            assert_eq!(body.id, graph.nodes[body.index].id);
        }
        assert_eq!(bodies.iter(app.world()).count(), 4);
        assert!(!app.world().resource::<PhysicsEditor>().running);
    }
    fn start(app: &mut App) {
        app.world_mut()
            .resource_scope(|world, mut state: Mut<PhysicsEditor>| {
                state.enabled = true;
                state.begin(&mut world.resource_mut::<Scene>().0, &HashSet::from([0, 2]));
            });
        app.update();
        app.world_mut().resource_mut::<PhysicsEditor>().target = Vec2::new(400.0, 0.0);
        for _ in 0..100 {
            app.update();
        }
    }
    #[test]
    fn drag_pushes_neighbors_preserves_formation_and_stops_at_locked_obstacle() {
        let mut app = app();
        let before = app.world().resource::<Scene>().0.nodes.clone();
        start(&mut app);
        let nodes = &app.world().resource::<Scene>().0.nodes;
        assert!(nodes[1].rect[0] > 55.0, "neighbor must be pushed");
        assert!(
            (nodes[0].rect[0] - nodes[2].rect[0]).abs() < 0.05,
            "selection must keep formation"
        );
        assert!(nodes[0].rect[0] <= 160.1, "must not cross locked obstacle");
        assert_eq!(nodes[3], before[3]);
        app.world_mut().resource_mut::<PhysicsEditor>().release();
        for _ in 0..300 {
            app.update();
        }
        assert!(!app.world().resource::<PhysicsEditor>().running);
        let after = app.world().resource::<Scene>().0.nodes.clone();
        app.world_mut().resource_mut::<Scene>().0.undo();
        app.update();
        assert_eq!(app.world().resource::<Scene>().0.nodes, before);
        app.world_mut().resource_mut::<Scene>().0.redo();
        app.update();
        assert_eq!(app.world().resource::<Scene>().0.nodes, after);
    }
    #[test]
    fn cancelling_restores_every_pushed_node_and_stops_simulation() {
        let mut app = app();
        let before = app.world().resource::<Scene>().0.nodes.clone();
        start(&mut app);
        app.world_mut()
            .resource_scope(|world, mut state: Mut<PhysicsEditor>| {
                state.cancel(&mut world.resource_mut::<Scene>().0);
            });
        for _ in 0..20 {
            app.update();
        }
        assert_eq!(app.world().resource::<Scene>().0.nodes, before);
        assert!(!app.world().resource::<Scene>().0.history.can_undo());
    }
    #[test]
    fn pointer_motion_reaches_the_rendered_selection_in_the_same_frame() {
        for hz in [30.0, 60.0, 120.0, 240.0] {
            for target in [Vec2::new(-24.0, -12.0), Vec2::new(-2400.0, -1200.0)] {
                let mut app = app();
                app.insert_resource(TimeUpdateStrategy::ManualDuration(
                    std::time::Duration::from_secs_f64(1.0 / hz),
                ));
                app.world_mut()
                    .resource_scope(|world, mut state: Mut<PhysicsEditor>| {
                        state.enabled = true;
                        state.begin(&mut world.resource_mut::<Scene>().0, &HashSet::from([0]));
                    });
                app.update();
                app.update();
                let origin = center(&app.world().resource::<Scene>().0.nodes[0]);
                // Canvas input runs in the Egui pass in PostUpdate, after the
                // fixed loop. Include frames faster than the old 120 Hz tick.
                app.add_systems(
                    PostUpdate,
                    (move |mut state: ResMut<PhysicsEditor>| state.target = target)
                        .in_set(bevy_egui::EguiPostUpdateSet::EndPass),
                );
                app.update();
                let state = app.world().resource::<PhysicsEditor>();
                let node = &app.world().resource::<Scene>().0.nodes[0];
                let actual = visual_center(state, 0, node) - origin;
                assert!(
                    actual.distance(target) < 0.01,
                    "{hz} Hz: pointer target {target:?}, visible displacement {actual:?}",
                );
            }
        }
    }

    #[test]
    fn direct_drag_releases_at_the_exact_target_with_one_undo() {
        let mut app = app();
        let before = app.world().resource::<Scene>().0.nodes.clone();
        app.world_mut()
            .resource_scope(|world, mut state: Mut<PhysicsEditor>| {
                state.enabled = true;
                state.begin(&mut world.resource_mut::<Scene>().0, &HashSet::from([0, 2]));
            });
        app.update();
        app.update();
        assert!(!app.world().resource::<PhysicsEditor>().reset);
        let target = Vec2::new(-10.0, -8.0);
        app.world_mut().resource_mut::<PhysicsEditor>().target = target;
        app.update();
        let offset = app.world().resource::<PhysicsEditor>().offset;
        assert!(offset.distance(target) < 0.001);
        app.world_mut().resource_mut::<PhysicsEditor>().release();
        for _ in 0..30 {
            app.update();
            let offset = app.world().resource::<PhysicsEditor>().offset;
            assert!(offset.x >= target.x - 0.001 && offset.y >= target.y - 0.001);
        }
        assert!(
            app.world()
                .resource::<PhysicsEditor>()
                .offset
                .distance(target)
                < 0.001
        );
        for _ in 0..120 {
            app.update();
        }
        assert!(!app.world().resource::<PhysicsEditor>().running);
        let after = app.world().resource::<Scene>().0.nodes.clone();
        for i in [0, 2] {
            assert!((center(&after[i]) - center(&before[i])).distance(target) < 0.001);
        }
        app.world_mut().resource_mut::<Scene>().0.undo();
        app.update();
        assert_eq!(app.world().resource::<Scene>().0.nodes, before);
        assert!(!app.world().resource::<Scene>().0.history.can_undo());
        app.world_mut().resource_mut::<Scene>().0.redo();
        app.update();
        assert_eq!(app.world().resource::<Scene>().0.nodes, after);
    }

    #[test]
    fn reversing_a_drag_reaches_the_new_target_on_the_next_frame() {
        let mut app = app();
        app.world_mut()
            .resource_scope(|world, mut state: Mut<PhysicsEditor>| {
                state.enabled = true;
                state.begin(&mut world.resource_mut::<Scene>().0, &HashSet::from([0]));
            });
        app.update();
        app.update();
        assert!(!app.world().resource::<PhysicsEditor>().reset);
        app.world_mut().resource_mut::<PhysicsEditor>().target = Vec2::new(-10.0, 0.0);
        app.update();
        let before = app.world().resource::<PhysicsEditor>().offset.x;
        app.world_mut().resource_mut::<PhysicsEditor>().target = Vec2::ZERO;
        app.update();
        let after = app.world().resource::<PhysicsEditor>().offset.x;
        assert!(after > before && after.abs() < 0.001);
        app.world_mut()
            .resource_scope(|world, mut state: Mut<PhysicsEditor>| {
                state.cancel(&mut world.resource_mut::<Scene>().0);
            });
        app.update();
        assert_eq!(app.world().resource::<Scene>().0.nodes[0].rect[0], 0.0);
    }
    #[test]
    fn direct_drag_reaches_the_pointer_at_different_zoom_levels() {
        let mut offsets = Vec::new();
        for scale in [0.2, 1.0, 4.0] {
            let mut app = app();
            app.world_mut()
                .resource_scope(|world, mut state: Mut<PhysicsEditor>| {
                    state.enabled = true;
                    state.drag_scale = scale;
                    state.begin(&mut world.resource_mut::<Scene>().0, &HashSet::from([0]));
                });
            app.update();
            app.update();
            assert!(!app.world().resource::<PhysicsEditor>().reset);
            app.world_mut().resource_mut::<PhysicsEditor>().target = Vec2::new(-10.0 * scale, 0.0);
            app.update();
            offsets.push(app.world().resource::<PhysicsEditor>().offset.x / scale);
        }
        assert!((offsets[0] + 10.0).abs() < 0.001);
        assert!(
            offsets
                .iter()
                .all(|offset| (offset - offsets[0]).abs() < 0.001)
        );
    }
    #[test]
    fn locked_or_empty_selection_never_starts_a_transaction() {
        let mut app = app();
        for selection in [HashSet::new(), HashSet::from([3])] {
            app.world_mut()
                .resource_scope(|world, mut state: Mut<PhysicsEditor>| {
                    state.enabled = true;
                    state.begin(&mut world.resource_mut::<Scene>().0, &selection);
                    assert!(!state.dragging && !state.running);
                });
        }
        assert!(!app.world().resource::<Scene>().0.history.can_undo());
    }
    #[test]
    fn disabled_physics_has_no_bodies_and_reenabling_uses_current_document() {
        let mut app = app();
        assert_eq!(
            app.world_mut()
                .query::<&BodyNode>()
                .iter(app.world())
                .count(),
            0
        );
        start(&mut app);
        assert_eq!(
            app.world_mut()
                .query::<&BodyNode>()
                .iter(app.world())
                .count(),
            4
        );
        app.world_mut()
            .resource_scope(|world, mut state: Mut<PhysicsEditor>| {
                state.toggle(&mut world.resource_mut::<Scene>().0);
            });
        app.update();
        assert_eq!(
            app.world_mut()
                .query::<&BodyNode>()
                .iter(app.world())
                .count(),
            0
        );
        app.world_mut().resource_mut::<Scene>().0.nodes[0].rect[0] = -100.0;
        app.world_mut().resource_mut::<Scene>().0.nodes[0].rect[2] = 80.0;
        app.world_mut()
            .resource_scope(|world, mut state: Mut<PhysicsEditor>| {
                state.toggle(&mut world.resource_mut::<Scene>().0);
            });
        app.update();
        let mut query = app.world_mut().query::<(&BodyNode, &Position)>();
        let (body, position) = query
            .iter(app.world())
            .find(|(body, _)| body.index == 0)
            .unwrap();
        assert_eq!(body.size.x, 80.0);
        assert_eq!(
            position.0,
            center(&app.world().resource::<Scene>().0.nodes[0])
        );
    }
    #[test]
    fn unrelated_sleeping_body_stays_asleep_during_a_drag() {
        let mut app = app();
        app.world_mut().resource_mut::<PhysicsEditor>().enabled = true;
        app.update();
        let mut remote = app.world().resource::<Scene>().0.nodes[0].clone();
        remote.id = "remote".into();
        remote.rect[0] = 10000.0;
        app.world_mut()
            .resource_mut::<Scene>()
            .0
            .nodes
            .push(remote.clone());
        let entity = app
            .world_mut()
            .spawn((
                BodyNode {
                    index: 4,
                    id: remote.id.clone(),
                    size: Vec2::splat(40.0),
                    members: vec![4],
                    locked: false,
                },
                RigidBody::Dynamic,
                Collider::rectangle(40.0, 40.0),
                Position(center(&remote)),
                Transform::from_translation(center(&remote).extend(0.0)),
                CollisionLayers::new(NORMAL, u32::MAX),
                Sleeping,
            ))
            .id();
        app.update();
        app.world_mut().entity_mut(entity).insert(Sleeping);
        start(&mut app);
        assert!(app.world().get::<Sleeping>(entity).is_some());
        assert_eq!(app.world().resource::<Scene>().0.nodes[4], remote);
    }

    // Run the document part of viewer::fit_groups every frame, including during
    // a pending physics transaction. Rendering resources are unnecessary here.
    fn fit_group_bounds(mut scene: ResMut<Scene>) {
        scene.0.refresh_group_bounds().unwrap();
    }

    fn group_drag_app(document: Document) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_secs_f64(1.0 / 120.0),
            ))
            .insert_resource(Scene(document))
            .init_resource::<Interaction>()
            .insert_resource(Config {
                font: vec![],
                benchmark: false,
                output: "unused".into(),
                screenshot: None,
                light: false,
                ui_scale: 1.0,
                physics: enabled_from_args(&[]),
                no_ui: false,
                preview: false,
                edit_node: None,
                diagnostics: false,
            });
        install(&mut app);
        app.add_systems(PostUpdate, fit_group_bounds.before(synchronize));
        app.finish();
        app.cleanup();
        app.update();
        assert!(app.world().resource::<PhysicsEditor>().enabled);
        app
    }

    fn collision_group() -> (Document, usize) {
        let mut doc = stress::synthetic(4, false, false, false).unwrap();
        for (node, rect) in doc.nodes.iter_mut().zip([
            [100.0, 0.0, 40.0, 40.0],
            [180.0, 80.0, 40.0, 40.0],
            [140.0, 40.0, 10.0, 10.0],
            [-160.0, 0.0, 40.0, 40.0],
        ]) {
            node.rect = rect;
        }
        doc.nodes[2].kind = "ConnectPoint".into();
        let inner = doc.create_group(&HashSet::from([0, 1, 2])).unwrap();
        let outer = doc.create_group(&HashSet::from([inner])).unwrap();
        (doc, outer)
    }

    #[test]
    fn collapsed_collision_tracks_projection_and_restores_expanded_size() {
        let (mut doc, group) = collision_group();
        doc.set_sections_collapsed(&HashSet::from([group]), true)
            .unwrap();
        let mut app = group_drag_app(doc);
        let original = app.world().resource::<Scene>().0.nodes[group].rect;
        let mut display = sections::Display::new(true);
        display.graph = Some(Graph {
            nodes: app.world().resource::<Scene>().0.nodes.clone(),
            edges: vec![],
            attachments: default(),
            counts: default(),
        });
        display.graph.as_mut().unwrap().nodes[group].rect[2..4].copy_from_slice(&[80.0, 40.0]);
        app.insert_resource(display);
        for size in [Vec2::new(80.0, 40.0), Vec2::new(120.0, 64.0)] {
            app.world_mut()
                .resource_mut::<sections::Display>()
                .graph
                .as_mut()
                .unwrap()
                .nodes[group]
                .rect[2..4]
                .copy_from_slice(&size.to_array());
            app.update();
            let world = app.world_mut();
            let mut query = world.query::<(&BodyNode, &Position)>();
            let (body, position) = query
                .iter(world)
                .find(|(body, _)| body.index == group)
                .unwrap();
            assert_eq!(body.size, size);
            assert_eq!(
                position.0,
                center(&world.resource::<Scene>().0.nodes[group])
            );
            assert_eq!(world.resource::<Scene>().0.nodes[group].rect, original);
        }
        app.world_mut().resource_mut::<sections::Display>().graph = None;
        app.update();
        let world = app.world_mut();
        let mut query = world.query::<&BodyNode>();
        let body = query.iter(world).find(|body| body.index == group).unwrap();
        assert_eq!(body.size, Vec2::new(original[2], original[3]));
    }

    #[test]
    fn external_text_and_nested_group_push_each_other_without_breaking_membership() {
        for drag_group in [false, true] {
            let (doc, group) = collision_group();
            let before = doc.nodes.clone();
            let mut app = group_drag_app(doc);
            app.world_mut()
                .resource_scope(|world, mut state: Mut<PhysicsEditor>| {
                    state.begin(
                        &mut world.resource_mut::<Scene>().0,
                        &HashSet::from([if drag_group { group } else { 3 }]),
                    );
                    state.target = Vec2::new(if drag_group { -350.0 } else { 350.0 }, 0.0);
                });
            for _ in 0..120 {
                app.update();
            }
            app.world_mut().resource_mut::<PhysicsEditor>().release();
            for _ in 0..300 {
                app.update();
            }
            let doc = &app.world().resource::<Scene>().0;
            let delta = center(&doc.nodes[group]) - center(&before[group]);
            assert!(delta.x.abs() > 20.0, "group did not move: {delta:?}");
            if drag_group {
                assert!(doc.nodes[3].rect[0] < before[3].rect[0] - 20.0);
            }
            assert!(doc.nodes[3].rect[0] + doc.nodes[3].rect[2] <= doc.nodes[group].rect[0] + 2.0);
            for i in [0, 1, 2, 4, 5] {
                assert_translated(&before[i..=i], &doc.nodes[i..=i], delta);
            }
            let after = doc.nodes.clone();
            app.world_mut().resource_mut::<Scene>().0.undo();
            app.update();
            assert_eq!(app.world().resource::<Scene>().0.nodes, before);
            app.world_mut().resource_mut::<Scene>().0.redo();
            app.update();
            assert_eq!(app.world().resource::<Scene>().0.nodes, after);
        }
    }

    #[test]
    fn moving_inside_group_ignores_ancestor_frames_and_cancel_restores_layout() {
        let (doc, _) = collision_group();
        let before = doc.nodes.clone();
        let mut app = group_drag_app(doc);
        app.world_mut()
            .resource_scope(|world, mut state: Mut<PhysicsEditor>| {
                state.begin(&mut world.resource_mut::<Scene>().0, &HashSet::from([0]));
                state.target = Vec2::new(-30.0, 0.0);
            });
        for _ in 0..120 {
            app.update();
        }
        let doc = &app.world().resource::<Scene>().0;
        assert!((doc.nodes[0].rect[0] - before[0].rect[0] + 30.0).abs() < 0.05);
        assert_eq!(&doc.nodes[1..4], &before[1..4]);
        app.world_mut()
            .resource_scope(|world, mut state: Mut<PhysicsEditor>| {
                state.cancel(&mut world.resource_mut::<Scene>().0);
            });
        app.update();
        assert_eq!(app.world().resource::<Scene>().0.nodes, before);
    }

    #[test]
    fn locked_member_keeps_whole_group_fixed_when_hit_from_outside() {
        let (mut doc, group) = collision_group();
        doc.nodes[0].locked = true;
        let before = doc.nodes.clone();
        let mut app = group_drag_app(doc);
        app.world_mut()
            .resource_scope(|world, mut state: Mut<PhysicsEditor>| {
                state.begin(&mut world.resource_mut::<Scene>().0, &HashSet::from([3]));
                state.target = Vec2::new(500.0, 0.0);
            });
        for _ in 0..120 {
            app.update();
        }
        let doc = &app.world().resource::<Scene>().0;
        for i in [0, 1, 2, 4, 5] {
            assert_eq!(doc.nodes[i], before[i]);
        }
        assert!(doc.nodes[3].rect[0] > before[3].rect[0]);
        assert!(doc.nodes[3].rect[0] + doc.nodes[3].rect[2] <= doc.nodes[group].rect[0] + 0.1);
    }

    #[test]
    fn selecting_a_child_of_a_locked_group_cannot_start_a_drag() {
        let (mut doc, group) = collision_group();
        doc.nodes[group].locked = true;
        let before = doc.nodes.clone();
        let mut state = PhysicsEditor::default();
        state.begin(&mut doc, &HashSet::from([0, 2]));
        assert!(!state.running);
        assert!(!doc.in_transaction());
        assert_eq!(doc.nodes, before);
    }

    fn assert_translated(before: &[GraphNode], after: &[GraphNode], delta: Vec2) {
        assert_eq!(before.len(), after.len());
        for (before, after) in before.iter().zip(after) {
            let expected = [
                before.rect[0] + delta.x,
                before.rect[1] - delta.y,
                before.rect[2],
                before.rect[3],
            ];
            for (actual, expected) in after.rect.iter().zip(expected) {
                assert!(
                    (*actual - expected).abs() < 0.05,
                    "{} {} moved out of formation: {:?}, expected {:?}",
                    after.kind,
                    after.id,
                    after.rect,
                    expected
                );
            }
            let mut normalized = after.clone();
            normalized.rect = before.rect;
            assert_eq!(&normalized, before);
        }
    }

    fn group_everything_and_drag(mut doc: Document) -> App {
        doc.refresh_group_bounds().unwrap();
        let original = doc.nodes.clone();
        let edges = doc.edges.clone();
        let group = doc
            .create_group(&(0..original.len()).collect::<HashSet<_>>())
            .unwrap();
        assert_eq!(group, original.len());
        // Selecting parents and descendants together must retain every old
        // Section's membership rather than leave empty frames outside the group.
        assert_eq!(&doc.nodes[..group], &original);
        assert_eq!(
            doc.expand_selection(&HashSet::from([group])),
            (0..doc.nodes.len()).collect()
        );
        let before = doc.nodes.clone();
        let mut app = group_drag_app(doc);
        app.world_mut()
            .resource_scope(|world, mut state: Mut<PhysicsEditor>| {
                state.begin(
                    &mut world.resource_mut::<Scene>().0,
                    &HashSet::from([group]),
                );
            });
        app.update();
        // Several motion samples catch accumulating offsets in passive Sections
        // and ConnectPoints as well as separation between selected rigid bodies.
        let mut target = Vec2::ZERO;
        for next in [Vec2::new(90.0, -60.0), Vec2::new(-120.0, 170.0)] {
            target = next;
            app.world_mut().resource_mut::<PhysicsEditor>().target = target;
            for _ in 0..30 {
                app.update();
            }
            assert_translated(&before, &app.world().resource::<Scene>().0.nodes, target);
            assert_eq!(app.world().resource::<Scene>().0.edges, edges);
        }
        app.world_mut().resource_mut::<PhysicsEditor>().release();
        for _ in 0..300 {
            app.update();
        }
        assert!(!app.world().resource::<PhysicsEditor>().running);
        assert!(app.world().resource::<Time<Physics>>().is_paused());
        let after = app.world().resource::<Scene>().0.nodes.clone();
        assert_translated(&before, &after, target);
        app.world_mut().resource_mut::<Scene>().0.undo();
        app.update();
        assert_eq!(app.world().resource::<Scene>().0.nodes, before);
        assert_eq!(app.world().resource::<Scene>().0.edges, edges);
        app.world_mut().resource_mut::<Scene>().0.redo();
        app.update();
        assert_eq!(app.world().resource::<Scene>().0.nodes, after);
        app
    }

    #[test]
    fn whole_nested_group_drag_keeps_sections_connectpoints_and_overlaps_together() {
        let node = |id: &str, kind: &str, x: f32, children: Vec<String>| GraphNode {
            id: id.into(),
            kind: kind.into(),
            rect: [x, 0.0, 40.0, 40.0],
            text: id.into(),
            url: None,
            font_size: 20.0,
            auto_size: false,
            color: [0.0; 4],
            attachment: None,
            source_name: None,
            children,
            locked: false,
            collapsed: false,
            stroke: vec![],
        };
        let _app = group_everything_and_drag(Document::memory(Graph {
            nodes: vec![
                node("a", "TextNode", 0.0, vec![]),
                node("b", "TextNode", 10.0, vec![]),
                node("point", "ConnectPoint", 200.0, vec![]),
                node("inner", "Section", 0.0, vec!["a".into(), "b".into()]),
                node(
                    "outer",
                    "Section",
                    0.0,
                    vec!["inner".into(), "point".into()],
                ),
            ],
            edges: vec![GraphEdge {
                id: "edge".into(),
                members: vec![0, 2],
                ..Default::default()
            }],
            attachments: Default::default(),
            counts: Default::default(),
        }));
    }

    #[test]
    #[ignore = "requires pinned tutorial from run.sh"]
    fn tutorial_whole_group_physics_drag_preserves_all_members_edges_and_saved_layout() {
        let fixture =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/tutorial-shortcut-keys.mio");
        let doc = Document::open(&fixture).unwrap();
        let node_count = doc.nodes.len();
        assert!(node_count >= 393);
        assert!(doc.nodes.iter().filter(|n| n.kind == "Section").count() >= 27);
        assert!(
            doc.nodes
                .iter()
                .filter(|n| n.kind == "ConnectPoint")
                .count()
                >= 21
        );
        let mut app = group_everything_and_drag(doc);
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("whole-group-drag.mio");
        app.world_mut()
            .resource_mut::<Scene>()
            .0
            .save(&output)
            .unwrap();
        let loaded = Document::open(&output).unwrap();
        let saved = &app.world().resource::<Scene>().0;
        assert_eq!(loaded.nodes.len(), node_count + 1);
        for node in &saved.nodes {
            let actual = loaded.nodes.iter().find(|n| n.id == node.id).unwrap();
            assert_translated(
                std::slice::from_ref(node),
                std::slice::from_ref(actual),
                Vec2::ZERO,
            );
        }
        assert_eq!(loaded.edges.len(), saved.edges.len());
        for edge in &saved.edges {
            let actual = loaded.edges.iter().find(|e| e.id == edge.id).unwrap();
            assert_eq!(
                edge.members
                    .iter()
                    .map(|&i| &saved.nodes[i].id)
                    .collect::<Vec<_>>(),
                actual
                    .members
                    .iter()
                    .map(|&i| &loaded.nodes[i].id)
                    .collect::<Vec<_>>()
            );
            let mut normalized = actual.clone();
            normalized.members.clone_from(&edge.members);
            assert_eq!(&normalized, edge);
        }
        assert_eq!(loaded.attachments, saved.attachments);
        assert_eq!(loaded.counts, saved.counts);
    }

    #[test]
    #[ignore = "requires desktop tutorial-main-3.1 and 3.2 MIO files"]
    fn desktop_tutorial_groups_keep_members_together_after_drag_undo_and_redo() {
        for name in ["tutorial-main-3.1.mio", "tutorial-main-3.2.mio"] {
            let mut doc = Document::open(&PathBuf::from("/home/waya/Desktop").join(name)).unwrap();
            assert!(doc.nodes.len() > 600);
            // Whole-document regrouping requires an unlocked in-memory copy.
            // Locked-group behavior is covered by the collision tests above.
            for node in &mut doc.nodes {
                node.locked = false;
            }
            let _app = group_everything_and_drag(doc);
        }
    }
}
