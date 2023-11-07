struct VertexOutput {
    @location(0) tex_coord: vec2<f32>,
    @builtin(position) position: vec4<f32>,
    @location(1) position2: vec4<f32>,
};

@group(0)
@binding(0)
var<uniform> transform: mat4x4<f32>;

@vertex
fn vs_main(
    @location(0) position: vec4<f32>,
    @location(1) tex_coord: vec2<f32>,
) -> VertexOutput {
    var result: VertexOutput;
    result.tex_coord = tex_coord;
    result.position = transform * position;
    result.position2 = position;
    return result;
}

@fragment
fn fs_main(vertex: VertexOutput) -> @location(0) vec4<f32> {
    //return vec4<f32>(vertex.tex_coord.x, vertex.tex_coord.y, 0.5, 1.0);
    var result = vertex.position2;
    result += 1.0;
    result *= 1.0 / 4.2;
    result.w = 1.0;
    return result;
}