// The package's interface: what a page building sliders and showing errors relies on.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { test } from "node:test";

const { Photo, Recipe, effects, looks, parameters } = createRequire(import.meta.url)("../../pkg/bullterrier_image_wasm.js");

const grey = (width, height) => new Photo(new Uint8Array(width * height * 4).fill(128), width, height);
const halftone = `[[step]]\neffect = "halftone"\ncell = 5\n`;

test("effects and their parameters are listed, for sliders", () => {
  assert.ok(effects().includes("halftone"));
  const cell = parameters("halftone").find((parameter) => parameter.name === "cell");
  assert.equal(cell.kind, "float");
  assert.ok(cell.min > 0 && cell.max > cell.min && cell.default >= cell.min && cell.default <= cell.max);
  assert.ok(cell.meaning.length > 0);
  assert.ok(looks().includes("crt"));
});

test("a recipe written as JSON renders as the same recipe written as TOML", () => {
  const fromToml = Recipe.fromToml(halftone);
  const fromJson = Recipe.fromJson(fromToml.toJson());
  const photo = grey(40, 30);
  assert.deepEqual(fromJson.render(photo).data.slice(0, 64), fromToml.render(photo).data.slice(0, 64));
  assert.equal(fromJson.toJson(), fromToml.toJson());
});

test("an alpha channel on the photo is not read: the render is opaque", () => {
  const out = Recipe.fromToml(halftone).render(grey(20, 20));
  assert.ok(out.data.every((byte, index) => index % 4 !== 3 || byte === 255));
});

const failure = (call) => {
  try {
    call();
  } catch (error) {
    return error;
  }
  assert.fail("expected an error");
};

test("an error is an Error named by its kind, with the core's message", () => {
  const unknown = failure(() => Recipe.fromToml(`[[step]]\neffect = "halftne"\n`).render(grey(8, 8)));
  assert.equal(unknown.name, "UnknownEffectError");
  assert.match(unknown.message, /halftone/); // the core's suggestion
  const range = failure(() => Recipe.fromToml(`[[step]]\neffect = "halftone"\ncell = -3\n`).render(grey(8, 8)));
  assert.equal(range.name, "RecipeError");
  assert.match(range.message, /cell/);
  assert.equal(failure(() => Recipe.fromToml("not = [toml")).name, "RecipeError");
  assert.equal(failure(() => Recipe.look("nope")).name, "RecipeError");
  assert.equal(failure(() => parameters("nope")).name, "UnknownEffectError");
});

test("a recipe that needs the subject says so when none was handed in", () => {
  const masked = `[[step]]\neffect = "halftone"\nmask = "subject"\n`;
  assert.equal(failure(() => Recipe.fromToml(masked).render(grey(16, 16))).name, "SubjectNotFoundError");
});

test("a photo or map of the wrong size is refused", () => {
  assert.equal(failure(() => new Photo(new Uint8Array(10), 4, 4)).name, "InvalidImageError");
  assert.equal(failure(() => grey(4, 4).setMap("mask", new Float32Array(3))).name, "MapNotFoundError");
  assert.equal(failure(() => grey(4, 4).setMap("mask", new Float32Array(16).fill(2))).name, "MapNotFoundError");
});

test("a size that is not a whole number of pixels is refused", () => {
  for (const size of [0, -4, 2.5, "big", null]) {
    assert.throws(() => Recipe.fromToml(halftone).render(grey(8, 8), { size }), TypeError, `size ${size}`);
  }
});
