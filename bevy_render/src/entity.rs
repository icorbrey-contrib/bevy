use crate::{
    mesh::Mesh, shader::uniforms::StandardMaterial, ActiveCamera, ActiveCamera2d, Camera,
    CameraType, Light, Renderable,
};
use bevy_asset::Handle;
use bevy_derive::EntityArchetype;
use bevy_transform::components::{LocalToWorld, Rotation, Scale, Translation};

#[derive(EntityArchetype, Default)]
#[module(meta = false)]
pub struct MeshEntity {
    // #[tag]
    pub mesh: Handle<Mesh>,
    // #[tag]
    pub material: Handle<StandardMaterial>,
    pub renderable: Renderable,
    pub local_to_world: LocalToWorld,
    pub translation: Translation,
    pub rotation: Rotation,
    pub scale: Scale,
}

#[derive(EntityArchetype, Default)]
#[module(meta = false)]
pub struct MeshMaterialEntity<T: Default + Send + Sync + 'static> {
    pub mesh: Handle<Mesh>,
    pub material: Handle<T>,
    pub renderable: Renderable,
    pub local_to_world: LocalToWorld,
    pub translation: Translation,
    pub rotation: Rotation,
    pub scale: Scale,
}

#[derive(EntityArchetype, Default)]
#[module(meta = false)]
pub struct LightEntity {
    pub light: Light,
    pub local_to_world: LocalToWorld,
    pub translation: Translation,
    pub rotation: Rotation,
}

#[derive(EntityArchetype, Default)]
#[module(meta = false)]
pub struct CameraEntity {
    pub camera: Camera,
    pub active_camera: ActiveCamera,
    pub local_to_world: LocalToWorld,
}

#[derive(EntityArchetype)]
#[module(meta = false)]
pub struct Camera2dEntity {
    pub camera: Camera,
    pub active_camera_2d: ActiveCamera2d,
}

impl Default for Camera2dEntity {
    fn default() -> Self {
        Camera2dEntity {
            camera: Camera::new(CameraType::default_orthographic()),
            active_camera_2d: ActiveCamera2d,
        }
    }
}
