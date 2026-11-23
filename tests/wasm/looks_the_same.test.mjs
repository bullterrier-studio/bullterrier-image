// The comparison tests/wasm judges renders with: a pair it must accept and pairs it must refuse.
import assert from "node:assert/strict";
import { test } from "node:test";

import { differenceInLooks } from "./looks_the_same.mjs";

// 1,000 opaque RGBA pixels: 3,000 colour samples, so three of them may be two levels off.
const base = () => new Uint8Array(4000).map((_, at) => (at % 4 === 3 ? 255 : 100));
const moved = (...changes) => {
  const other = base();
  for (const [at, levels] of changes) other[at] += levels;
  return other;
};

test("samples one level off and one in a thousand colour samples two levels off look the same", () => {
  assert.equal(differenceInLooks(moved([1, 1], [2, -1], [500, 1], [998, 1]), base()), null);
  assert.equal(differenceInLooks(moved([0, 2], [4, 2], [8, 2]), base()), null);
});

test("a sample three levels off does not look the same", () => {
  assert.match(differenceInLooks(moved([7, 3]), base()), /3 levels apart/);
});

test("more than a thousandth of the colour samples two levels off does not look the same", () => {
  // four in 3,000: the alpha bytes do not widen the share
  assert.match(differenceInLooks(moved([0, 2], [4, 2], [8, 2], [12, 2]), base()), /4 samples are 2 levels apart; 3 allowed/);
});

test("pictures of other lengths do not look the same", () => {
  assert.match(differenceInLooks(base().subarray(4), base()), /samples against/);
});
