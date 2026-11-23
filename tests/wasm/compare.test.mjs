// The package in Node renders what the native extension renders: the same size and pixels that look the same
// (looks_the_same.mjs; never deepEqual on a big buffer: a mismatch builds a diff of the whole value). oracle.py wrote the
// answers.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { test } from "node:test";

import { differenceInLooks } from "./looks_the_same.mjs";

const here = new URL(".", import.meta.url);
const { Photo, Recipe } = createRequire(import.meta.url)("../../pkg/bullterrier_image_wasm.js");
const oracle = JSON.parse(readFileSync(new URL("../../pkg/oracle.json", here)));

const { width, height } = oracle.photo;
const photo = new Photo(readFileSync(new URL("../../pkg/photo.rgba", here)), width, height);
const maps = {};
for (const name of ["subject", "depth"]) {
  const bytes = readFileSync(new URL(`../../pkg/${name}.f32`, here));
  maps[name] = new Float32Array(bytes.buffer, bytes.byteOffset, bytes.length / 4);
}

for (const [name, expected] of Object.entries(oracle.cases)) {
  const { recipe, look, size } = expected;
  test(`${name}: pixels that look the same as the native render`, () => {
    const loaded = look ? Recipe.look(look) : Recipe.fromToml(readFileSync(new URL(recipe, here), "utf8"));
    if (expected.error) {
      assert.throws(() => loaded.render(photo, { size, maps }), { name: expected.error });
      return;
    }
    const out = loaded.render(photo, { size, maps });
    assert.deepEqual([out.width, out.height], [expected.width, expected.height]);
    assert.equal(out.data.length, out.width * out.height * 4);
    const native = readFileSync(new URL(`../../pkg/${name}.rgba`, here));
    assert.equal(differenceInLooks(out.data, native), null);
  });
}
