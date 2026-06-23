use std::{collections::HashMap, path::Path};

use anyhow::{Context, Error};
use block::{BlockRegistry, BlockTexture};
use wgpu::{
    AddressMode, Device, Extent3d, FilterMode, Origin3d, Queue, Sampler, SamplerDescriptor,
    TexelCopyBufferLayout, TexelCopyTextureInfo, Texture, TextureAspect, TextureDescriptor,
    TextureDimension, TextureFormat, TextureUsages, TextureView, TextureViewDescriptor,
    TextureViewDimension,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaterialTextures(pub(crate) [u32; 6]);

pub struct TextureArray {
    texture: Texture,
    view: TextureView,
    sampler: Sampler,
    layer_count: u32,
}

impl TextureArray {
    pub fn texture(&self) -> &Texture {
        &self.texture
    }
    pub fn view(&self) -> &TextureView {
        &self.view
    }
    pub fn sampler(&self) -> &Sampler {
        &self.sampler
    }
    pub fn layer_count(&self) -> u32 {
        self.layer_count
    }
}

pub fn build_texture_array(
    device: &Device,
    queue: &Queue,
    registry: &BlockRegistry,
) -> Result<(TextureArray, Vec<MaterialTextures>), Error> {
    let mut layer_of = HashMap::new();
    let mut paths = Vec::new();

    let mut intern = |path: &'static Path| {
        *layer_of.entry(path).or_insert_with(|| {
            paths.push(path);
            (paths.len() - 1) as u32
        })
    };

    for block in registry.iter() {
        match block.texture() {
            BlockTexture::Uniform(p) => {
                intern(p);
            }
            BlockTexture::Directional(paths) => {
                for path in paths {
                    intern(path);
                }
            }
        }
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

    let materials = registry
        .iter()
        .map(|block| match block.texture() {
            BlockTexture::Uniform(p) => {
                let layer = layer_of[p];
                MaterialTextures([layer; 6])
            }
            BlockTexture::Directional(materials) => {
                MaterialTextures(materials.map(|path| layer_of[path]))
            }
        })
        .collect();

    Ok((
        TextureArray {
            texture,
            view,
            sampler,
            layer_count,
        },
        materials,
    ))
}
