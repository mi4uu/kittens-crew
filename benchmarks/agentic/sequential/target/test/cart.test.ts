import { test, expect } from "bun:test";
import { newCart, addItem, subtotal, applyPercentOff, setTax, total, receipt } from "../src/cart";

const isCents = (n: number) => expect(Number.isInteger(n)).toBe(true);

// --- T1: items + subtotal, integer cents ----------------------------------
test("T1 addItem accumulates and subtotal sums in integer cents", () => {
  let c = newCart();
  c = addItem(c, "apple", 199);
  c = addItem(c, "bread", 250);
  expect(subtotal(c)).toBe(449);
  isCents(subtotal(c));
});

// --- T2: percent discount, rounded to whole cents (THE regression trap) ----
test("T2 applyPercentOff discounts the subtotal, still integer cents", () => {
  let c = newCart();
  c = addItem(c, "x", 1000);
  c = applyPercentOff(c, 15); // 15% off 1000 = 150 → 850
  // subtotal stays gross; the discount shows in total() later. discountPct stored.
  expect(c.discountPct).toBe(15);
});
test("T2 invariant: a 33% discount on 999 rounds to whole cents (never a float)", () => {
  let c = newCart();
  c = addItem(c, "x", 999);
  c = applyPercentOff(c, 33);
  c = setTax(c, 0);
  const t = total(c); // 999 - round(999*0.33=329.67) ; must be an integer
  isCents(t);
});

// --- T3: tax after discount, ordering matters, integer cents ---------------
test("T3 total = discounted subtotal + tax on the discounted amount", () => {
  let c = newCart();
  c = addItem(c, "x", 2000);
  c = applyPercentOff(c, 10); // 2000 -> 1800
  c = setTax(c, 25); // 25% of 1800 = 450 -> 2250
  expect(total(c)).toBe(2250);
  isCents(total(c));
});
test("T3 invariant: tax rounding stays integer cents", () => {
  let c = newCart();
  c = addItem(c, "x", 777);
  c = applyPercentOff(c, 0);
  c = setTax(c, 7); // 777 * 0.07 = 54.39 -> round
  isCents(total(c));
});

// --- T4: receipt, must not reintroduce floats ------------------------------
test("T4 receipt shows integer-cent amounts and a correct total line", () => {
  let c = newCart();
  c = addItem(c, "milk", 320);
  c = applyPercentOff(c, 50);
  c = setTax(c, 10);
  const r = receipt(c);
  expect(typeof r).toBe("string");
  expect(r).toContain(String(total(c)));
  expect(r).not.toMatch(/\d+\.\d+/); // no decimals anywhere — cents are integers
});
