// The comparison tests/wasm judges renders with: a pair it must accept and pairs it must refuse.
import assert from "node:assert/strict";
import { test } from "node:test";

import { differenceInLooks } from "./looks_the_same.mjs";

const base = () => new Uint8Array(1000).fill(100);
const moved = (...changes) => {
  const other = base();
  for (const [at, levels] of changes) other[at] += levels;
  return other;
};

test("samples one level off and one in a thousand two levels off look the same", () => {
  assert.equal(differenceInLooks(moved([1, 1], [2, -1], [500, 1], [999, 1]), base()), null);
  assert.equal(differenceInLooks(moved([5, 2]), base()), null);
});

test("a sample three levels off does not look the same", () => {
  assert.match(differenceInLooks(moved([7, 3]), base()), /3 levels apart/);
});

test("more than a thousandth of the samples two levels off does not look the same", () => {
  assert.match(differenceInLooks(moved([5, 2], [6, 2]), base()), /2 levels apart/);
});

test("pictures of other lengths do not look the same", () => {
  assert.match(differenceInLooks(base().subarray(1), base()), /samples against/);
});
