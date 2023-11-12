struct VertexOutput {
    @location(0) tex_coord: vec2<f32>,
    @location(1) normal: vec3<f32>,
    @builtin(position) position: vec4<f32>,
};

@group(0) @binding(0) var<uniform> transform: mat4x4<f32>;
@group(0) @binding(1) var<uniform> player_chunk: vec3<i32>;
@group(0) @binding(2) var t_diffuse: texture_2d<f32>;
@group(0) @binding(3) var s_diffuse: sampler;

@group(1) @binding(0) var<uniform> chunk_position: vec3<i32>;

@vertex
fn vs_main(
    @location(0) position: vec4<f32>,
    @location(1) tex_coord: vec2<f32>,
    @location(2) face_index: u32,
) -> VertexOutput {
    var result: VertexOutput;
    result.tex_coord = tex_coord;
    result.normal = normal(face_index);

    // this allows vertices to be relative to their chunk to avoid precision issues for large coordinates
    var offset = vec4<i32>(chunk_position - player_chunk, 0);

    result.position = transform * (position + vec4<f32>(offset));
    return result;
}

@fragment
fn fs_main(vertex: VertexOutput) -> @location(0) vec4<f32> {
    // parameters:
    let ambient_strength = 0.2;
    let direction_strength = 0.8;
    let to_light = normalize(vec3<f32>(-0.1, 1.0, 0.2));
    let light_color = vec3<f32>(1.0, 1.0, 1.0);

    let ambient = ambient_strength * light_color;

    let normal = normalize(vertex.normal); // currently all normals should already be normalized, but do it anyway
    let diffuse = max(dot(normal, to_light), 0.0) * direction_strength * light_color;

    var object_color = textureSample(t_diffuse, s_diffuse, vertex.tex_coord).xyz;

    let border = -1.0f; // disabled for now
    if any(vertex.tex_coord < vec2<f32>(border, border)) || any(vertex.tex_coord > vec2<f32>(1.0, 1.0) - border) {
        object_color = vec3<f32>(0.02, 0.02, 0.02);
    }

    let result = (ambient + diffuse) * object_color;

    return vec4<f32>(result, 1.0);
}

fn normal(face_index: u32) -> vec3<f32> {
  switch face_index {
    case 0u: { return vec3<f32>( 1.0, 0.0, 0.0);}
    case 1u: { return vec3<f32>(-1.0, 0.0, 0.0);}
    case 2u: { return vec3<f32>( 0.0, 1.0, 0.0);}
    case 3u: { return vec3<f32>( 0.0,-1.0, 0.0);}
    case 4u: { return vec3<f32>( 0.0, 0.0, 1.0);}
    case 5u: { return vec3<f32>( 0.0, 0.0,-1.0);}
    default: { return vec3<f32>( 0.0, 0.0, 0.0);} // unreachable, could be collapsed with case 5
  }
}
