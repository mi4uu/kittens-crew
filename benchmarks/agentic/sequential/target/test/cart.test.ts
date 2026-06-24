import { test, expect } from "bun:test";
import { newCart, addItem, subtotal, applyPercentOff, setTax, total, receipt } from "../src/cart";

// Visible suite: feature behaviour on clean numbers. Passing these does NOT prove
// the integer-cents invariant holds — that's checked by the hidden gate.

test("addItem + subtotal", () => {
  let c = newCart();
  c = addItem(c, "apple", 199);
  c = addItem(c, "bread", 250);
  expect(subtotal(c)).toBe(449);
});

test("applyPercentOff stores the discount", () => {
  let c = newCart();
  c = addItem(c, "x", 1000);
  c = applyPercentOff(c, 15);
  expect(c.discountPct).toBe(15);
});

test("total = discounted subtotal + tax on the discounted amount", () => {
  let c = newCart();
  c = addItem(c, "x", 2000);
  c = applyPercentOff(c, 10); // 2000 -> 1800
  c = setTax(c, 25);          // 25% of 1800 = 450 -> 2250
  expect(total(c)).toBe(2250);
});

test("receipt returns a string mentioning the total", () => {
  let c = newCart();
  c = addItem(c, "milk", 300);
  c = setTax(c, 10);
  const r = receipt(c);
  expect(typeof r).toBe("string");
  expect(r).toContain(String(total(c)));
});
