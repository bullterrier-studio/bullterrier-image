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
  const masked = Recipe.fromToml(`[[step]]\neffect = "halftone"\nmask = "subject"\n`);
  assert.equal(failure(() => new Photo(new Uint8Array(10), 4, 4)).name, "InvalidImageError");
  const wrong = (subject) => failure(() => masked.render(grey(4, 4), { maps: { subject } })).name;
  assert.equal(wrong(new Float32Array(3)), "SubjectNotFoundError");
  assert.equal(wrong(new Float32Array(16).fill(2)), "SubjectNotFoundError");
});

test("a map is handed in with each render and kept for none", () => {
  const masked = Recipe.fromToml(`[[step]]\neffect = "halftone"\nmask = "subject"\n`);
  const photo = grey(16, 16);
  const subject = new Float32Array(256).fill(1);
  assert.equal(masked.render(photo, { maps: { subject } }).width, 16);
  assert.equal(failure(() => masked.render(photo)).name, "SubjectNotFoundError");
  assert.equal(failure(() => masked.render(photo, { maps: { subject: [1, 1] } })).name, "TypeError");
  assert.match(failure(() => masked.render(photo, { maps: { subject: [1, 1] } })).message, /maps\.subject/);
  assert.match(failure(() => masked.render(photo, { maps: 3 })).message, /maps/);
});

test("a render is a plain object whose data is a JavaScript array to keep", () => {
  const out = Recipe.fromToml(halftone).render(grey(8, 8));
  assert.equal(Object.getPrototypeOf(out), Object.prototype);
  assert.deepEqual(Object.keys(out).sort(), ["data", "height", "width"]);
  assert.ok(out.data instanceof Uint8ClampedArray);
  assert.equal(out.data, out.data);
  assert.equal(out.free, undefined);
});

test("the numbers of a recipe are listed by address and set by address, as in Python", () => {
  const recipe = Recipe.fromToml(halftone);
  const cell = recipe.parameters()["halftone.cell"];
  assert.equal(cell.value, 5);
  assert.ok(cell.min > 0 && cell.max > cell.min && cell.meaning.length > 0 && typeof cell.unit === "string");
  const wider = recipe.withValues({ "halftone.cell": 9 });
  assert.equal(wider.parameters()["halftone.cell"].value, 9);
  assert.equal(recipe.parameters()["halftone.cell"].value, 5); // the recipe itself is left as it is
  assert.deepEqual(JSON.parse(wider.toJson()), JSON.parse(Recipe.fromToml(halftone.replace("cell = 5", "cell = 9")).toJson()));
  assert.equal(failure(() => recipe.withValues({ "halftone.cell": -1 })).name, "RecipeError");
  assert.match(failure(() => recipe.withValues({ "halftone.cell": -1 })).message, /halftone\.cell/);
  assert.equal(failure(() => recipe.withValues({ nope: 1 })).name, "RecipeError");
  assert.match(failure(() => recipe.withValues({ "halftone.cell": true })).message, /halftone\.cell/);
  assert.equal(failure(() => recipe.withValues({ "halftone.cell": true })).name, "TypeError");
  assert.equal(failure(() => recipe.withValues({ "halftone.cell": "9" })).name, "RecipeError"); // text is a colour
  assert.equal(failure(() => recipe.withValues(4)).name, "TypeError");
});

test("a colour is listed with kind color and set by address in any spelling", () => {
  const lamp = Recipe.fromJson(JSON.stringify({ steps: [{ effect: "point_light" }, { effect: "bloom" }] }));
  const color = lamp.parameters()["point_light.color"];
  assert.deepEqual([color.kind, color.value, color.unit, color.min, color.max, color.nullable], ["color", "#ffffff", "", null, null, false]);
  assert.equal(lamp.parameters()["bloom.color"].nullable, true);
  assert.equal(lamp.parameters()["bloom.color"].value, null);
  assert.equal(lamp.parameters()["point_light.gain"].kind, "number");
  const set = [{ "point_light.color": "#ff8800" }, { "point_light.color": "#f80" }, { "point_light.color": [1, 136 / 255, 0] }]
    .map((values) => lamp.withValues(values));
  for (const other of set) assert.equal(other.toJson(), set[0].toJson());
  assert.equal(set[0].parameters()["point_light.color"].value, "#ff8800");
  assert.match(failure(() => lamp.withValues({ "point_light.color": "#12" })).message, /point_light\.color.*not a colour/);
  assert.match(failure(() => lamp.withValues({ "point_light.color": 0.5 })).message, /point_light\.color.*is a colour/);
});

test("a recipe is checked when it is loaded", () => {
  const unknown = failure(() => Recipe.fromToml(`${halftone}zzz = 1\n`));
  assert.equal(unknown.name, "RecipeError");
  assert.match(unknown.message, /zzz/);
  assert.equal(failure(() => Recipe.fromJson("{}")).name, "RecipeError");
  assert.equal(failure(() => Recipe.fromToml(`[[step]]\neffect = "halftne"\n`)).name, "UnknownEffectError");
});

test("a photo takes whole positive sizes and 8-bit bytes only", () => {
  for (const [width, height] of [[2.5, 4], [4, 2.5], [2 ** 32 + 2, 4], [0, 4], [4, -1], ["4", 4], [NaN, 4]]) {
    const error = failure(() => new Photo(new Uint8Array(64), width, height));
    assert.equal(error.name, "TypeError", `${width} x ${height}`);
    assert.match(error.message, /width|height/);
  }
  for (const rgba of [new Float32Array(64), new Array(64).fill(0), new ArrayBuffer(64), null, "x"]) {
    const error = failure(() => new Photo(rgba, 4, 4));
    assert.equal(error.name, "TypeError");
    assert.match(error.message, /rgba/);
  }
  assert.equal(Recipe.fromToml(halftone).render(new Photo(new Uint8ClampedArray(64).fill(9), 4, 4)).width, 4);
});

test("options that are not an object are refused, naming options", () => {
  for (const options of [5, "big", true]) {
    const error = failure(() => Recipe.fromToml(halftone).render(grey(8, 8), options));
    assert.equal(error.name, "TypeError");
    assert.match(error.message, /options/);
  }
});

test("recipes that need a depth or a file fail as named in the browser", () => {
  const depth = `[[step]]\neffect = "halftone"\nmask = "depth"\n`;
  assert.equal(failure(() => Recipe.fromToml(depth).render(grey(8, 8))).name, "DepthNotFoundError");
});

test("a size that is not a whole number of pixels is refused", () => {
  for (const size of [0, -4, 2.5, "big", null]) {
    assert.throws(() => Recipe.fromToml(halftone).render(grey(8, 8), { size }), TypeError, `size ${size}`);
  }
});
