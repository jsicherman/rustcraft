struct Camera {
    view_proj: mat4x4<f32>,
}

struct ObjectUniform {
    transform: mat4x4<f32>,
    mat_layers_01: vec4<u32>,
    mat_layers_02: vec4<u32>,
}

struct LightingUniform {
    sun_direction_and_strength: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> camera: Camera;

@group(1) @binding(0)
var<uniform> object: ObjectUniform;

@group(2) @binding(0)
var tex_array: texture_2d_array<f32>;
@group(2) @binding(1)
var samp: sampler;

@group(3) @binding(0)
var<uniform> lighting: LightingUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) material: u32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) material: u32,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * object.transform * vec4<f32>(in.position, 1.0);
    out.normal = in.normal;
    out.uv = in.uv;
    out.material = in.material;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let use_override = object.mat_layers_01[0] == 1u;
    let layer = select(in.material, object.mat_layers_01[1], use_override);
    let albedo = textureSampleLevel(tex_array, samp, in.uv, layer, 0.0);

    let sun_dir = normalize(lighting.sun_direction_and_strength.xyz);
    let sun_strength = lighting.sun_direction_and_strength.w;
    let direct_light = max(dot(normalize(in.normal), sun_dir), 0.0) * sun_strength;
    let ambient = mix(0.03, 0.10, sun_strength);
    let light = ambient + direct_light * (1.0 - ambient);

    return vec4<f32>(albedo.rgb * light, albedo.a);
}
