import assert from "node:assert/strict";
import test from "node:test";
import { greeting } from "../src/greeting.js";

test("creates a friendly greeting", () => {
  assert.equal(greeting("HAWK"), "Welcome, HAWK!");
});
