struct VertexOutput {
    @location(0) tex_coord: vec2<f32>,
    @location(1) normal: vec3<f32>,
    @builtin(position) position: vec4<f32>,
};

@group(0) @binding(0) var<uniform> transform: mat4x4<f32>;
@group(0) @binding(1) var<uniform> player_chunk: vec3<i32>;

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
    let to_light = normalize(vec3<f32>(-0.1, 1.0, 0.2));
    let light_color = vec3<f32>(1.0, 1.0, 1.0);

    let ambient = ambient_strength * light_color;

    let normal = normalize(vertex.normal); // currently all normals should already be normalized, but do it anyway
    let diffuse = max(dot(normal, to_light), 0.0) * light_color;

    var object_color = vec3<f32>(101.0, 132.0, 80.0) / 255.0;
    object_color = object_color * object_color;
    if any(vertex.tex_coord < vec2<f32>(0.05, 0.05)) || any(vertex.tex_coord > vec2<f32>(0.95, 0.95)) {
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
