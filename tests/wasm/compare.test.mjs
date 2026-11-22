// The package in Node renders what the native extension renders: the same size and the same bytes (within one level for
// the Fourier effects, see oracle.py), compared by SHA-256 (never deepEqual: a mismatch of a big buffer builds a diff of
// the whole value). oracle.py wrote the answers.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { test } from "node:test";

const here = new URL(".", import.meta.url);
const { Photo, Recipe } = createRequire(import.meta.url)("../../pkg/bullterrier_image_wasm.js");
const oracle = JSON.parse(readFileSync(new URL("../../pkg/oracle.json", here)));
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

const { width, height } = oracle.photo;
const photo = new Photo(readFileSync(new URL("../../pkg/photo.rgba", here)), width, height);
const maps = {};
for (const name of ["subject", "depth"]) {
  const bytes = readFileSync(new URL(`../../pkg/${name}.f32`, here));
  maps[name] = new Float32Array(bytes.buffer, bytes.byteOffset, bytes.length / 4);
}

for (const [name, expected] of Object.entries(oracle.cases)) {
  const { recipe, look, size } = expected;
  test(`${name}: the same pixels as the native render`, (t) => {
    const loaded = look ? Recipe.look(look) : Recipe.fromToml(readFileSync(new URL(recipe, here), "utf8"));
    if (expected.error) {
      assert.throws(() => loaded.render(photo, { size, maps }), { name: expected.error });
      return;
    }
    const out = loaded.render(photo, { size, maps });
    assert.deepEqual([out.width, out.height], [expected.width, expected.height]);
    assert.equal(out.data.length, out.width * out.height * 4);
    if (expected.bound > 0 && sha256(out.data) !== expected.sha256) {
      // The Fourier allowance (CLAUDE.md, WebAssembly; oracle.py FOURIER_EFFECTS): a recipe through rustfft may differ
      // from the native render by `bound` levels of a channel, never more. The largest difference and the count of
      // differing bytes are reported, so a CI log is the measurement.
      const native = readFileSync(new URL(`../../pkg/${name}.rgba`, here));
      let [worst, differing] = [0, 0];
      for (let index = 0; index < native.length; index++) {
        const gap = Math.abs(native[index] - out.data[index]);
        worst = Math.max(worst, gap);
        differing += gap > 0 ? 1 : 0;
      }
      const measured = `${differing} of ${native.length} bytes differ, by at most ${worst} levels`;
      t.diagnostic(measured);
      assert.ok(worst <= expected.bound, `${measured}; the bound is ${expected.bound}`);
    } else {
      assert.equal(sha256(out.data), expected.sha256);
    }
  });
}
