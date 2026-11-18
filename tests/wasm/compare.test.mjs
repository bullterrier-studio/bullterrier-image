// The package in Node renders what the native extension renders: the same size and the same bytes, compared by
// SHA-256 (never deepEqual: a mismatch of a big buffer builds a diff of the whole value). oracle.py wrote the answers.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { test } from "node:test";

const here = new URL(".", import.meta.url);
const { Photo, Recipe } = createRequire(import.meta.url)("../../pkg/bullterrier_image_wasm.js");
const oracle = JSON.parse(readFileSync(new URL("../../pkg/oracle.json", here)));
const cases = JSON.parse(readFileSync(new URL("cases.json", here)));
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

const { width, height } = oracle.photo;
const photo = new Photo(readFileSync(new URL("../../pkg/photo.rgba", here)), width, height);
const subject = readFileSync(new URL("../../pkg/subject.f32", here));
photo.setMap("subject", new Float32Array(subject.buffer, subject.byteOffset, subject.length / 4));

for (const { name, recipe, look, size } of cases) {
  test(`${name}: the same pixels as the native render`, () => {
    const loaded = look ? Recipe.look(look) : Recipe.fromToml(readFileSync(new URL(recipe, here), "utf8"));
    const out = loaded.render(photo, size === undefined ? undefined : { size });
    const expected = oracle.cases[name];
    assert.deepEqual([out.width, out.height], [expected.width, expected.height]);
    assert.equal(out.data.length, out.width * out.height * 4);
    assert.equal(sha256(out.data), expected.sha256);
  });
}
