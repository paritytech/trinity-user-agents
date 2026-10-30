#version 300 es
// One textured quad with no vertex buffers: draw 4 vertices as a triangle strip.
// uRect = (left, top, right, bottom) in clip space. vUv (0,0) is the texture's top-left, row 0 of the file.
uniform vec4 uRect;
out vec2 vUv;
void main() {
  vec2 c = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));
  vUv = c;
  gl_Position = vec4(mix(uRect.x, uRect.z, c.x), mix(uRect.y, uRect.w, c.y), 0., 1.);
}
