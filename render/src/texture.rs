use std::{collections::HashMap, path::Path};

use anyhow::{Context, Error};
use wgpu::{
    AddressMode, BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout,
    BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingResource, BindingType, Device,
    Extent3d, FilterMode, Origin3d, Queue, Sampler, SamplerBindingType, SamplerDescriptor,
    ShaderStages, TexelCopyBufferLayout, TexelCopyTextureInfo, TextureAspect, TextureDescriptor,
    TextureDimension, TextureFormat, TextureSampleType, TextureUsages, TextureView,
    TextureViewDescriptor, TextureViewDimension,
};

pub type BlockScale = [[f32; 2]; 3];
pub type MaterialTextures = [u32; 6];

pub struct TextureArray {
    view: TextureView,
    sampler: Sampler,
}

impl TextureArray {
    pub fn view(&self) -> &TextureView {
        &self.view
    }
    pub fn sampler(&self) -> &Sampler {
        &self.sampler
    }
}

fn build_texture_array(
    device: &Device,
    queue: &Queue,
    block_resources: Vec<([&'static Path; 6], BlockScale)>,
    texture_resources: Vec<[&'static Path; 6]>,
) -> Result<(TextureArray, Vec<MaterialTextures>, Vec<BlockScale>), Error> {
    let mut layer_of = HashMap::new();
    let mut paths = Vec::new();

    let mut intern = |path: &'static Path| {
        *layer_of.entry(path).or_insert_with(|| {
            paths.push(path);
            (paths.len() - 1) as u32
        })
    };

    for texture in block_resources
        .iter()
        .flat_map(|(texture, _)| *texture)
        .chain(texture_resources.iter().flat_map(|texture| *texture))
    {
        intern(texture);
    }

    let mut images = Vec::with_capacity(paths.len());
    let mut dims = None;

    for path in &paths {
        let img = image::open(path)
            .with_context(|| format!("failed to load block texture {path:?}"))?
            .to_rgba8();
        let size = img.dimensions();

        match dims {
            None => dims = Some(size),
            Some(expected) => anyhow::ensure!(
                size == expected,
                "texture {path:?} is {}x{}, expected {}x{}",
                size.0,
                size.1,
                expected.0,
                expected.1
            ),
        }

        images.push(img);
    }

    let (width, height) = dims.context("block registry referenced no textures")?;
    let layer_count = paths.len() as u32;

    let texture = device.create_texture(&TextureDescriptor {
        label: Some("block texture array"),
        size: Extent3d {
            width,
            height,
            depth_or_array_layers: layer_count,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8UnormSrgb,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });

    for (layer, img) in images.iter().enumerate() {
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: Origin3d {
                    x: 0,
                    y: 0,
                    z: layer as u32,
                },
                aspect: TextureAspect::All,
            },
            img,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }

    let view = texture.create_view(&TextureViewDescriptor {
        dimension: Some(TextureViewDimension::D2Array),
        ..Default::default()
    });

    let sampler = device.create_sampler(&SamplerDescriptor {
        label: Some("block texture sampler"),
        mag_filter: FilterMode::Nearest,
        min_filter: FilterMode::Nearest,
        address_mode_u: AddressMode::Repeat,
        address_mode_v: AddressMode::Repeat,
        ..Default::default()
    });

    let materials = block_resources
        .iter()
        .map(|(texture, _)| match texture.len() {
            1 => [layer_of[texture[0]]; 6],
            6 => std::array::from_fn(|i| layer_of[texture[i]]),
            _ => unreachable!("block texture array must have either 1 or 6 textures"),
        })
        .collect();

    let scale_layers = block_resources.iter().map(|(_, scale)| *scale).collect();

    Ok((TextureArray { view, sampler }, materials, scale_layers))
}

#[allow(clippy::type_complexity)]
pub fn configure_textures(
    device: &Device,
    queue: &Queue,
    block_resources: Vec<([&'static Path; 6], BlockScale)>,
    texture_resources: Vec<[&'static Path; 6]>,
) -> (
    BindGroupLayout,
    BindGroup,
    Vec<MaterialTextures>,
    Vec<BlockScale>,
) {
    let texture_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("texture array bgl"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });

    let (textures, material_layers, scale_layers) =
        build_texture_array(device, queue, block_resources, texture_resources).unwrap();

    let texture_bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("texture array bind group"),
        layout: &texture_bgl,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(textures.view()),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::Sampler(textures.sampler()),
            },
        ],
    });

    (
        texture_bgl,
        texture_bind_group,
        material_layers,
        scale_layers,
    )
}
