struct VertexOutput {
    @location(0) tex_coord: vec2<f32>,
    @builtin(position) position: vec4<f32>,
};

@group(0) @binding(0) var<uniform> transform: mat4x4<f32>;
@group(0) @binding(1) var<uniform> player_chunk: vec3<i32>;

@group(1) @binding(0) var<uniform> chunk_position: vec3<i32>;

@vertex
fn vs_main(
    @location(0) position: vec4<f32>,
    @location(1) tex_coord: vec2<f32>,
) -> VertexOutput {
    var result: VertexOutput;
    result.tex_coord = tex_coord;

    // this allows vertices to be relative to their chunk to avoid precision issues for large coordinates
    var offset = vec4<i32>(chunk_position - player_chunk, 0);

    result.position = transform * (position + vec4<f32>(offset));
    return result;
}

@fragment
fn fs_main(vertex: VertexOutput) -> @location(0) vec4<f32> {
    if any(vertex.tex_coord < vec2<f32>(0.05, 0.05)) || any(vertex.tex_coord > vec2<f32>(0.95, 0.95)) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    return vec4<f32>(vertex.tex_coord.x * 0.1, mix(0.4, 1.0, vertex.tex_coord.y), 0.3, 1.0);
}