struct Camera {
    view_proj: mat4x4<f32>,
}

struct SkyboxUniform {
    sky_color: vec4<f32>,
    sun_direction: vec4<f32>,
    moon_params: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> camera: Camera;

@group(1) @binding(0)
var<uniform> skybox: SkyboxUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.clip_position.z = out.clip_position.w;
    out.world_pos = in.position;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let dir = normalize(in.world_pos);
    let sun_dir = normalize(skybox.sun_direction.xyz);
    let moon_dir = -sun_dir;
    
    let base_color = skybox.sky_color.xyz;
    let sun_intensity = skybox.sun_direction.w;
    let moon_intensity = skybox.moon_params.x;
    let horizon_blend = max(0.0, dir.y) * 0.5 + 0.5;
    let darkened = base_color * 0.8;

    let sun_dot = dot(dir, sun_dir);
    let sun_disk = smoothstep(0.998, 0.9996, sun_dot);
    let sun_glow = smoothstep(0.96, 0.998, sun_dot);
    let sun_color = vec3<f32>(1.0, 0.92, 0.72) * sun_intensity;

    let moon_dot = dot(dir, moon_dir);
    let moon_disk = smoothstep(0.99945, 0.9999, moon_dot);
    let moon_glow = smoothstep(0.993, 0.99945, moon_dot);
    let moon_color = vec3<f32>(0.78, 0.82, 0.9) * moon_intensity;

    let sky = mix(darkened, base_color, horizon_blend);
    let color = sky
        + sun_color * (sun_disk + sun_glow * 0.25)
        + moon_color * (moon_disk + moon_glow * 0.05);
    
    return vec4<f32>(color, 1.0);
}
