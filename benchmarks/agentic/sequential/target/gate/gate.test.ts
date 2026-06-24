import { test, expect } from "bun:test";
import { newCart, addItem, applyPercentOff, setTax, total, receipt } from "../src/cart";

// HIDDEN GATE — the agent never sees this during the run. It checks the
// integer-cents invariant (stated ONCE, in task 1, never repeated) on tricky
// rounding values that a float implementation gets wrong. Run only at the end.
// Passing the visible suite but failing this = the agent forgot the invariant.
const isCents = (n: number) => expect(Number.isInteger(n)).toBe(true);

test("GATE 33% off 999 stays whole cents", () => {
  let c = newCart(); c = addItem(c, "x", 999); c = applyPercentOff(c, 33); c = setTax(c, 0);
  isCents(total(c));
});
test("GATE 7% tax on 777 stays whole cents", () => {
  let c = newCart(); c = addItem(c, "x", 777); c = applyPercentOff(c, 0); c = setTax(c, 7);
  isCents(total(c));
});
test("GATE receipt shows no decimal points", () => {
  let c = newCart(); c = addItem(c, "milk", 333); c = applyPercentOff(c, 50); c = setTax(c, 7);
  expect(receipt(c)).not.toMatch(/\d+\.\d+/);
});
