use glam::{Quat, Vec3};
use rapier3d::prelude::*;

pub const FIXED_DT: f32 = 1.0 / 60.0;
pub const WORLD: u32 = 1;
pub const PROP: u32 = 2;
pub const PLAYER: u32 = 4;

pub fn vector(v: Vec3) -> Vector<Real> {
    vector![v.x, v.y, v.z]
}
pub fn vec3(v: &Vector<Real>) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}
pub fn rotation(q: Quat) -> Rotation<Real> {
    Rotation::from_quaternion(rapier3d::na::Quaternion::new(q.w, q.x, q.y, q.z))
}
pub fn quaternion(q: &Rotation<Real>) -> Quat {
    let q = q.quaternion();
    Quat::from_xyzw(q.i, q.j, q.k, q.w)
}
fn groups(category: u32, filter: u32) -> InteractionGroups {
    InteractionGroups::new(
        Group::from_bits_truncate(category),
        Group::from_bits_truncate(filter),
    )
}

pub enum Shape {
    Box(Vec3),
    Sphere(f32),
    Cylinder(f32, f32),
    Cone(f32, f32),
    Compound(Vec<(Vec3, Vec3)>),
}

pub struct Physics {
    pub bodies: RigidBodySet,
    pub colliders: ColliderSet,
    pipeline: PhysicsPipeline,
    islands: IslandManager,
    broad: BroadPhaseMultiSap,
    narrow: NarrowPhase,
    joints: ImpulseJointSet,
    multi_joints: MultibodyJointSet,
    ccd: CCDSolver,
    query: QueryPipeline,
    pub impacts: Vec<(u128, Vec3, f32)>,
}

impl Default for Physics {
    fn default() -> Self {
        Self {
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            pipeline: PhysicsPipeline::new(),
            islands: IslandManager::new(),
            broad: BroadPhaseMultiSap::new(),
            narrow: NarrowPhase::new(),
            joints: ImpulseJointSet::new(),
            multi_joints: MultibodyJointSet::new(),
            ccd: CCDSolver::new(),
            query: QueryPipeline::new(),
            impacts: Vec::new(),
        }
    }
}

impl Physics {
    pub fn add(
        &mut self,
        shape: Shape,
        position: Vec3,
        q: Quat,
        mass: f32,
        friction: f32,
        tag: u128,
    ) -> RigidBodyHandle {
        let dynamic = mass > 0.0;
        let builder = if dynamic {
            RigidBodyBuilder::dynamic()
        } else {
            RigidBodyBuilder::fixed()
        };
        let body = self.bodies.insert(
            builder
                .translation(vector(position))
                .rotation(rotation(q).scaled_axis())
                .linear_damping(0.09)
                .angular_damping(0.18)
                .ccd_enabled(dynamic)
                .user_data(tag)
                .build(),
        );
        let collider = match shape {
            Shape::Box(h) => ColliderBuilder::cuboid(h.x, h.y, h.z),
            Shape::Sphere(r) => ColliderBuilder::ball(r),
            Shape::Cylinder(r, h) => ColliderBuilder::cylinder(h * 0.5, r),
            Shape::Cone(r, h) => ColliderBuilder::cone(h * 0.5, r),
            Shape::Compound(parts) => ColliderBuilder::compound(
                parts
                    .into_iter()
                    .map(|(p, h)| {
                        (
                            Isometry::translation(p.x, p.y, p.z),
                            SharedShape::cuboid(h.x, h.y, h.z),
                        )
                    })
                    .collect(),
            ),
        };
        let collider = collider
            .friction(friction)
            .restitution(0.08)
            .mass(mass.max(0.001))
            .collision_groups(groups(
                if dynamic { PROP } else { WORLD },
                WORLD | PROP | PLAYER,
            ))
            .user_data(tag)
            .build();
        self.colliders
            .insert_with_parent(collider, body, &mut self.bodies);
        body
    }

    pub fn player(&mut self, position: Vec3) -> RigidBodyHandle {
        let body = self.bodies.insert(
            RigidBodyBuilder::dynamic()
                .translation(vector(position))
                .locked_axes(LockedAxes::ROTATION_LOCKED)
                .ccd_enabled(true)
                .build(),
        );
        self.colliders.insert_with_parent(
            ColliderBuilder::capsule_y(0.49, 0.30)
                .mass(78.0)
                .friction(0.0)
                .restitution(0.0)
                .collision_groups(groups(PLAYER, WORLD | PROP))
                .build(),
            body,
            &mut self.bodies,
        );
        body
    }

    pub fn step(&mut self, dt: f32) {
        let velocities: std::collections::HashMap<_, _> = self
            .bodies
            .iter()
            .filter(|(_, body)| body.is_dynamic())
            .map(|(handle, body)| (handle, *body.linvel()))
            .collect();
        let params = IntegrationParameters {
            dt,
            num_solver_iterations: std::num::NonZeroUsize::new(8).unwrap(),
            ..Default::default()
        };
        self.pipeline.step(
            &vector![0.0, -9.81, 0.0],
            &params,
            &mut self.islands,
            &mut self.broad,
            &mut self.narrow,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.joints,
            &mut self.multi_joints,
            &mut self.ccd,
            Some(&mut self.query),
            &(),
            &(),
        );
        self.impacts.clear();
        for pair in self.narrow.contact_pairs() {
            if !pair.has_any_active_contact {
                continue;
            }
            let a = &self.colliders[pair.collider1];
            let b = &self.colliders[pair.collider2];
            let impulse: f32 = pair
                .manifolds
                .iter()
                .flat_map(|m| &m.points)
                .map(|p| p.data.impulse)
                .sum();
            let velocity = |collider: &Collider| {
                collider
                    .parent()
                    .and_then(|handle| velocities.get(&handle).copied())
                    .unwrap_or_default()
            };
            let relative = velocity(a) - velocity(b);
            let approach = pair
                .manifolds
                .iter()
                .map(|m| relative.dot(&m.data.normal).abs())
                .fold(0.0f32, f32::max);
            // Resting support impulses must not become a stream of impact sounds.
            if impulse > 4.0 && approach > 0.75 {
                let tag = if a.user_data > 0 {
                    a.user_data
                } else {
                    b.user_data
                };
                let point = pair
                    .manifolds
                    .iter()
                    .flat_map(|m| &m.data.solver_contacts)
                    .next()
                    .map(|contact| contact.point.coords)
                    .unwrap_or(a.position().translation.vector);
                self.impacts.push((tag, vec3(&point), impulse));
            }
        }
    }

    pub fn refresh_queries(&mut self) {
        self.bodies
            .propagate_modified_body_positions_to_colliders(&mut self.colliders);
        self.query.update(&self.colliders);
    }

    pub fn raycast(
        &self,
        origin: Vec3,
        dir: Vec3,
        max: f32,
        exclude: RigidBodyHandle,
        props_only: bool,
    ) -> Option<(RigidBodyHandle, f32)> {
        let ray = Ray::new(
            point![origin.x, origin.y, origin.z],
            vector(dir.normalize()),
        );
        let filter = QueryFilter::default()
            .exclude_rigid_body(exclude)
            .groups(groups(PLAYER, if props_only { PROP } else { WORLD | PROP }));
        self.query
            .cast_ray(&self.bodies, &self.colliders, &ray, max, true, filter)
            .and_then(|(c, distance)| self.colliders[c].parent().map(|body| (body, distance)))
    }

    pub fn hold_filter(&mut self, body: RigidBodyHandle, held: bool) {
        let handles = self.bodies[body].colliders().to_vec();
        for c in handles {
            self.colliders[c]
                .set_collision_groups(groups(PROP, WORLD | PROP | if held { 0 } else { PLAYER }));
        }
        self.refresh_queries();
    }

    pub fn spring(&mut self, body: RigidBodyHandle, target: Vec3) {
        let b = &mut self.bodies[body];
        let delta = target - vec3(b.translation());
        let acceleration =
            (delta * 115.0 - vec3(b.linvel()) * 19.0 + Vec3::Y * 9.81).clamp_length_max(180.0);
        b.reset_forces(true);
        b.add_force(vector(acceleration * b.mass()), true);
        b.set_angvel(*b.angvel() * 0.93, true);
    }

    pub fn release(&mut self, body: RigidBodyHandle, throw: Option<Vec3>) {
        self.hold_filter(body, false);
        let b = &mut self.bodies[body];
        b.reset_forces(true);
        if let Some(velocity) = throw {
            b.set_linvel(vector(velocity), true);
        }
    }

    pub fn remove(&mut self, handle: RigidBodyHandle) {
        self.bodies.remove(
            handle,
            &mut self.islands,
            &mut self.colliders,
            &mut self.joints,
            &mut self.multi_joints,
            true,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gravity_pick_spring_throw_and_collision_filter() {
        let mut p = Physics::default();
        p.add(
            Shape::Box(Vec3::new(10.0, 0.2, 10.0)),
            Vec3::new(0.0, -0.2, 0.0),
            Quat::IDENTITY,
            0.0,
            0.7,
            0,
        );
        let player = p.player(Vec3::new(4.0, 1.0, 4.0));
        let b = p.add(
            Shape::Box(Vec3::splat(0.4)),
            Vec3::new(0.0, 2.0, 0.0),
            Quat::IDENTITY,
            12.0,
            0.6,
            1,
        );
        for _ in 0..180 {
            p.step(FIXED_DT);
        }
        assert!((p.bodies[b].translation().y - 0.4).abs() < 0.06);
        assert_eq!(
            p.raycast(
                Vec3::new(0.0, 2.0, 4.0),
                Vec3::new(0.0, -1.6, -4.0),
                8.0,
                player,
                false
            )
            .unwrap()
            .0,
            b
        );
        p.hold_filter(b, true);
        for _ in 0..120 {
            p.spring(b, Vec3::new(0.0, 2.5, 0.0));
            p.step(FIXED_DT);
        }
        assert!((p.bodies[b].translation().y - 2.5).abs() < 0.1);
        p.release(b, Some(Vec3::new(0.0, 2.0, -15.0)));
        assert!(p.bodies[b].linvel().z < -14.0);
        for &c in p.bodies[b].colliders() {
            assert!(p.colliders[c]
                .collision_groups()
                .filter
                .contains(Group::from_bits_truncate(PLAYER)));
        }
    }
}
