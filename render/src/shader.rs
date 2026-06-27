use wgpu::{
    Device, Extent3d, ShaderModule, ShaderModuleDescriptor, ShaderSource, Texture,
    TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
};
use winit::dpi::PhysicalSize;

pub fn configure_depth_shader(device: &Device, size: PhysicalSize<u32>) -> (Texture, ShaderModule) {
    let depth_texture = depth_texture(device, size.width, size.height);

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("voxel shader"),
        source: ShaderSource::Wgsl(include_str!("../shaders/shader.wgsl").into()),
    });

    (depth_texture, shader)
}

pub fn depth_texture(device: &Device, width: u32, height: u32) -> Texture {
    device.create_texture(&TextureDescriptor {
        label: Some("depth"),
        size: Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Depth32Float,
        usage: TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}
