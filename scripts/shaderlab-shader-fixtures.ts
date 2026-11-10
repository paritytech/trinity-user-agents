// Prints the shaders Shaderlab's own compile step writes for each material the
// core's shader tests use: rust/crates/truapi/tests/fixtures/materials/compiled.v9.json.
//
//   bun scripts/shaderlab-shader-fixtures.ts <shaderlab-spa checkout> > \
//     rust/crates/truapi/tests/fixtures/materials/compiled.v9.json
//
// The checkout must be on a commit of Shaderlab's main, with
// `bun install --ignore-scripts --filter @shaderlab/shader` run in it.

import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

const checkout = process.argv[2];
if (!checkout)
  throw new Error(
    "Usage: bun scripts/shaderlab-shader-fixtures.ts <shaderlab-spa checkout>",
  );

const { compileShaderModule, CORE_SHADER_COMPILER } = await import(
  join(checkout, "packages/shader/src/compiler.js")
);

const PARAM_KEYS = [
  "name",
  "label",
  "type",
  "default",
  "min",
  "max",
  "presentation",
];
const PRESENTATION_KEYS = ["version", "group", "help", "unit", "step"];

type Param = Record<string, unknown>;

function ordered(value: Record<string, unknown>, keys: string[]) {
  return Object.fromEntries(
    keys
      .filter((key) => value[key] !== undefined)
      .map((key) => [key, value[key]]),
  );
}

function paramJson(param: Param) {
  const value = ordered(param, PARAM_KEYS);
  if (param.presentation)
    value.presentation = ordered(
      param.presentation as Param,
      PRESENTATION_KEYS,
    );
  return JSON.stringify(value);
}

function normalizeBody(body: string) {
  return `${body.replace(/\r\n?/g, "\n").replace(/\s+$/, "")}\n`;
}

function materialText(params: Param[], body: string) {
  return [
    "// @material 1",
    ...params.map((param) => `// @control ${paramJson(param)}`),
    normalizeBody(body),
  ].join("\n");
}

/** A Studio source: `// @control` lines anywhere, the rest is the body. */
function fromStudioSource(source: string) {
  const params: Param[] = [];
  const body = source
    .split("\n")
    .filter((line) => {
      const control = /^\s*\/\/ @control (.+)$/.exec(line);
      if (control) params.push(JSON.parse(control[1]));
      return !control && !/^\s*\/\/\s*@controls\s*$/.test(line);
    })
    .join("\n");
  return { params, body };
}

/** The body and parameters back out of canonical material text. */
function fromMaterialText(text: string) {
  const lines = text.split("\n");
  const controls = lines
    .slice(1)
    .filter((line) => line.startsWith("// @control "));
  return {
    params: controls.map((line) =>
      JSON.parse(line.slice("// @control ".length)),
    ),
    body: lines.slice(1 + controls.length).join("\n"),
  };
}

const sampler = (name: string, type: string) => ({ name, label: name, type });

const synthetic: { name: string; params: Param[]; body: string }[] = [
  {
    name: "typed read inside a helper with a samplerVec2 parameter",
    params: [sampler("flow", "samplerVec2")],
    body: "vec2 pair(samplerVec2 image, vec2 point) {\n  return texture(image, point);\n}\nvec4 material(vec2 uv) {\n  return vec4(pair(sp_flow, uv), 0.0, 1.0);\n}\n",
  },
  {
    name: "qualified sampler parameter",
    params: [sampler("height", "samplerFloat")],
    body: "float height(in highp samplerFloat image, vec2 point) {\n  return texture(image, point);\n}\nvec4 material(vec2 uv) {\n  return vec4(height(sp_height, uv));\n}\n",
  },
  {
    name: "declaration without a body and a parameter without a name",
    params: [sampler("height", "samplerFloat")],
    body: "float height(samplerFloat, vec2);\nvec4 material(vec2 uv) {\n  return vec4(height(sp_height, uv));\n}\nfloat height(samplerFloat image, vec2 point) {\n  return texture(image, point);\n}\n",
  },
  {
    name: "sampler in parentheses",
    params: [sampler("height", "samplerFloat")],
    body: "vec4 material(vec2 uv) {\n  return vec4(texture((sp_height), uv));\n}\n",
  },
  {
    name: "nested sampling calls",
    params: [sampler("flow", "samplerVec2")],
    body: "vec4 material(vec2 uv) {\n  return texture(r_input, texture(sp_flow, uv));\n}\n",
  },
  {
    name: "every sampling function",
    params: [],
    body: [
      "vec4 material(vec2 uv) {",
      "  vec4 sum = texture(r_input, uv);",
      "  sum += texture(r_input, uv, 0.5);",
      "  sum += textureLod(r_input, uv, 0.0);",
      "  sum += textureOffset(r_input, uv, ivec2(1, 0));",
      "  sum += textureProj(r_input, vec3(uv, 1.0));",
      "  sum += textureProjLod(r_input, vec4(uv, 0.0, 1.0), 0.0);",
      "  sum += textureGrad(r_input, uv, vec2(0.0), vec2(0.0));",
      "  sum += textureLodOffset(r_input, uv, 0.0, ivec2(0, 1));",
      "  sum += textureGradOffset(r_input, uv, vec2(0.0), vec2(0.0), ivec2(1, 1));",
      "  sum += textureProjOffset(r_input, vec3(uv, 1.0), ivec2(1, 0));",
      "  sum += textureProjGrad(r_input, vec3(uv, 1.0), vec2(0.0), vec2(0.0));",
      "  sum += texelFetch(r_input, ivec2(uv * vec2(textureSize(r_input, 0))), 0);",
      "  return sum / 12.0;",
      "}",
      "",
    ].join("\n"),
  },
  {
    name: "noise read as one channel",
    params: [],
    body: "vec4 material(vec2 uv) {\n  float grain = texture(r_noise, uv);\n  return texture(r_input, uv) * grain;\n}\n",
  },
  {
    name: "a user function passing a sampler on",
    params: [sampler("art", "samplerVec4")],
    body: "vec4 inner(sampler2D image, vec2 point) {\n  return texture(image, point);\n}\nvec4 outer(sampler2D image, vec2 point) {\n  return inner(image, point);\n}\nvec4 material(vec2 uv) {\n  return outer(r_base, uv) + outer(sp_art, uv);\n}\n",
  },
  {
    name: "a function returning an array, with a sampler parameter",
    params: [sampler("height", "samplerFloat")],
    body: "float[2] pair(samplerFloat image, vec2 point) {\n  return float[2](texture(image, point), 1.0);\n}\nvec4 material(vec2 uv) {\n  return vec4(pair(sp_height, uv)[0]);\n}\n",
  },
  {
    name: "samplerVec4 read without a swizzle",
    params: [sampler("art", "samplerVec4")],
    body: "vec4 material(vec2 uv) {\n  return texture(sp_art, uv);\n}\n",
  },
  {
    name: "comments and spaces around arguments",
    params: [sampler("height", "samplerFloat")],
    body: "float height(samplerFloat image /* map */ , vec2 point) {\n  return texture( image , point );\n}\nvec4 material(vec2 uv) {\n  return vec4(height( sp_height // the map\n    , uv ));\n}\n",
  },
];

const humanityDir = join(
  checkout,
  "apps/studio/src/features/projects/templates/assets/humanity",
);
const humanity = readdirSync(humanityDir)
  .filter((file) => file.endsWith(".glsl"))
  .sort()
  .map((file) => ({
    name: `humanity ${file}`,
    ...fromStudioSource(readFileSync(join(humanityDir, file), "utf8")),
  }));

const vectors = JSON.parse(
  readFileSync(
    join(
      import.meta.dir,
      "../rust/crates/truapi/tests/fixtures/materials/material-format.v1.json",
    ),
    "utf8",
  ),
);
const accepted = vectors.cases
  .filter((vector: { expect: { ok: boolean } }) => vector.expect.ok)
  .map((vector: { name: string; text: string }) => ({
    name: `format vector ${vector.name}`,
    ...fromMaterialText(vector.text),
  }));

function compile(
  name: string,
  params: Param[],
  body: string,
  floatLinearFallback: boolean,
) {
  const module = {
    version: 9,
    kind: "source",
    compiler: CORE_SHADER_COMPILER,
    target: "webgl2",
    approval: "experimental",
    parameters: [],
    resources: [],
    arguments: params,
    source: normalizeBody(body),
  };
  try {
    const { vertex, fragment } = compileShaderModule(module, {
      floatLinearFallback,
    });
    return { vertex, fragment };
  } catch (error) {
    throw new Error(
      `Shaderlab cannot compile ${name}: ${(error as Error).message}`,
    );
  }
}

const commit = execFileSync("git", ["-C", checkout, "rev-parse", "HEAD"], {
  encoding: "utf8",
}).trim();
const cases = [...humanity, ...accepted, ...synthetic].map(
  ({ name, params, body }) => ({
    name,
    text: materialText(params, body),
    normal: compile(name, params, body, false),
    fallback: compile(name, params, body, true),
  }),
);

console.log(
  JSON.stringify(
    {
      format: "shaderlab-compiled-v9",
      source: `paritytech/shaderlab-spa ${commit}: packages/shader/src/compiler.js compileShaderModule`,
      cases,
    },
    null,
    "\t",
  ),
);
