//! [`Rig`]: a creature as named bones, each placed on its parent and carrying the Armory parts
//! that move with it, built in the bone's own space. The game spawns one entity per bone and
//! sets its transform from a [`crate::pose`]; there is no skinning, so every part is rigid and
//! the bones overlap where they meet (a snake's segments, a rat's hips) so no gap opens.
//!
//! Each bone's main surface ([`SURFACE_PARTS`]) also gives it a [`Capsule`] ([`Bone::hit`]),
//! the body it presents to a blow, which moves with the bone as the pose does.

use armory::geom::vec3::V3;
use armory::{ItemMesh, Part};
use fb_body::Placement;

/// The names of the parts that are a bone's main surface (fur, skin, scales, a skeleton's bone), as opposed to
/// its details: eyes, whiskers, claws, teeth, tongue, markings and bellies lying on or inside
/// that surface.
pub const SURFACE_PARTS: [&str; 7] = ["fur", "spine", "head", "jaw", "tail", "paw", "bone"];

/// The points within `radius` of the segment from `a` to `b`: a body part as a blow finds it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Capsule {
    pub a: V3,
    pub b: V3,
    pub radius: f32,
}

impl Capsule {
    /// The capsule round `points` along the longest side of their bounding box: its radius the
    /// mean of the other two half sides, its ends that far in from the box's ends. Our own fit,
    /// close for the long, round trunks creatures are made of; `None` with no points.
    pub fn around(points: impl IntoIterator<Item = V3>) -> Option<Capsule> {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        let mut any = false;
        for p in points {
            any = true;
            for i in 0..3 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        }
        if !any {
            return None;
        }
        let half = [0, 1, 2].map(|i| 0.5 * (hi[i] - lo[i]));
        let centre = [0, 1, 2].map(|i| 0.5 * (hi[i] + lo[i]));
        let long = (0..3).max_by(|&i, &j| half[i].total_cmp(&half[j])).unwrap_or(0);
        let radius = 0.5 * (half.iter().sum::<f32>() - half[long]);
        let reach = (half[long] - radius).max(0.0);
        let (mut a, mut b) = (centre, centre);
        a[long] -= reach;
        b[long] += reach;
        Some(Capsule { a, b, radius })
    }

    /// How long its middle segment is.
    pub fn length(&self) -> f32 {
        armory::geom::vec3::distance(self.a, self.b)
    }
}

/// One bone of a [`Rig`].
#[derive(Clone, Debug, PartialEq)]
pub struct Bone {
    /// Its name (`"body"`, `"head"`, `"leg_front_left"`, `"spine_3"`...), unique in the rig.
    pub name: String,
    /// Index of the bone it hangs from, earlier in [`Rig::bones`]; `None` hangs from the
    /// creature's root.
    pub parent: Option<usize>,
    /// Where it sits on its parent at rest.
    pub rest: Placement,
    /// What moves with it, in its own space.
    pub mesh: ItemMesh,
}

impl Bone {
    /// The body this bone presents to a blow, in its own space: a [`Capsule`] round its main
    /// surface ([`SURFACE_PARTS`]), or `None` for a bone that is all detail (a snake's tongue).
    pub fn hit(&self) -> Option<Capsule> {
        Capsule::around(self.mesh.parts.iter().filter(|p| SURFACE_PARTS.contains(&p.name)).flat_map(|p| p.positions.iter().copied()))
    }
}

/// A creature's bones, parents before children.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rig {
    pub bones: Vec<Bone>,
}

impl Rig {
    /// Adds a bone and returns its index.
    pub fn add(&mut self, name: impl Into<String>, parent: Option<usize>, rest: Placement, parts: Vec<Part>) -> usize {
        let name = name.into();
        debug_assert!(self.index(&name).is_none(), "two bones named {name}");
        debug_assert!(parent.is_none_or(|p| p < self.bones.len()), "{name}'s parent comes after it");
        self.bones.push(Bone { name, parent, rest, mesh: ItemMesh { parts } });
        self.bones.len() - 1
    }

    /// The index of the bone called `name`.
    pub fn index(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|b| b.name == name)
    }

    pub fn bone(&self, name: &str) -> Option<&Bone> {
        self.bones.iter().find(|b| b.name == name)
    }

    /// Every bone's placement in creature space, given each bone's placement on its parent.
    pub fn world(&self, local: &[Placement]) -> Vec<Placement> {
        let mut world: Vec<Placement> = Vec::with_capacity(self.bones.len());
        for (bone, place) in self.bones.iter().zip(local) {
            let w = match bone.parent {
                Some(p) => world[p].then(place),
                None => *place,
            };
            world.push(w);
        }
        world
    }

    /// The rest placements, one per bone.
    pub fn rest(&self) -> Vec<Placement> {
        self.bones.iter().map(|b| b.rest).collect()
    }

    /// Every part placed in creature space by `local` (one placement per bone, on its parent),
    /// as one item: for bounds, tests and a still picture.
    pub fn posed_mesh(&self, local: &[Placement]) -> ItemMesh {
        let world = self.world(local);
        let mut out = ItemMesh::default();
        for (bone, place) in self.bones.iter().zip(&world) {
            for part in &bone.mesh.parts {
                let mut part = part.clone();
                part.transform(&place.rotation, place.origin);
                out.parts.push(part);
            }
        }
        out
    }

    /// [`Rig::posed_mesh`] at rest.
    pub fn rest_mesh(&self) -> ItemMesh {
        self.posed_mesh(&self.rest())
    }

    /// Checks every part's buffers ([`Part::validate`]) and every rotation is proper and
    /// orthonormal, so the game can turn it into a quaternion.
    pub fn validate(&self) -> Result<(), String> {
        for bone in &self.bones {
            for part in &bone.mesh.parts {
                part.validate().map_err(|e| format!("{}: {e}", bone.name))?;
            }
            let m = bone.rest.rotation;
            if (m.determinant() - 1.0).abs() > 1e-3 {
                return Err(format!("{}: rest rotation is not proper", bone.name));
            }
        }
        Ok(())
    }

    /// How many parts the rig draws.
    pub fn part_count(&self) -> usize {
        self.bones.iter().map(|b| b.mesh.parts.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Creature, Species};
    use armory::geom::vec3;

    #[test]
    fn a_capsule_runs_along_the_longest_side() {
        let points = [[-1.0, -5.0, -2.0], [1.0, 5.0, 2.0]];
        let c = Capsule::around(points).unwrap();
        assert_eq!(c.radius, 1.5);
        assert_eq!((c.a, c.b), ([0.0, -3.5, 0.0], [0.0, 3.5, 0.0]));
        assert!(Capsule::around(std::iter::empty()).is_none());
    }

    /// The distance from `p` to the nearest point of `c`'s middle segment.
    fn off(c: &Capsule, p: V3) -> f32 {
        let ab = vec3::sub(c.b, c.a);
        let t = (vec3::dot(vec3::sub(p, c.a), ab) / vec3::dot(ab, ab).max(1e-9)).clamp(0.0, 1.0);
        vec3::distance(p, vec3::add(c.a, vec3::scale(ab, t)))
    }

    #[test]
    fn every_bone_with_a_surface_has_a_capsule_round_it() {
        for species in Species::ALL {
            for seed in [1, 7, 1207] {
                let rig = Creature::of(species, seed).rig();
                for bone in &rig.bones {
                    let hit = bone.hit();
                    assert_eq!(hit.is_some(), bone.name != "tongue", "{species:?} {}", bone.name);
                    let Some(hit) = hit else { continue };
                    // Its surface lies round the capsule's skin, not far outside it.
                    let surface: Vec<V3> = bone.mesh.parts.iter().filter(|p| SURFACE_PARTS.contains(&p.name)).flat_map(|p| p.positions.clone()).collect();
                    let inside = surface.iter().filter(|&&p| off(&hit, p) <= 1.6 * hit.radius + 1e-3).count();
                    assert!(inside as f32 >= 0.9 * surface.len() as f32, "{species:?} {}: {inside} of {}", bone.name, surface.len());
                }
            }
        }
    }
}
