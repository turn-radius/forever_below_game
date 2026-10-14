//! [`Rig`]: a creature as named bones, each placed on its parent and carrying the Armory parts
//! that move with it, built in the bone's own space. The game spawns one entity per bone and
//! sets its transform from a [`crate::pose`]; there is no skinning, so every part is rigid and
//! the bones overlap where they meet (a snake's segments, a rat's hips) so no gap opens.

use armory::{ItemMesh, Part};
use fb_body::Placement;

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
