// A tiny money/cart module. THE INVARIANT: every monetary value is an integer
// number of cents — never a float. Discounts and tax must round to whole cents.
// Tasks build on each other; a later task that uses floats will regress the
// invariant tests an earlier task made pass. Implement the stubs; keep tests green.
export type Item = { name: string; cents: number };
export type Cart = { items: Item[]; discountPct: number; taxPct: number };

export const newCart = (): Cart => ({ items: [], discountPct: 0, taxPct: 0 });

// T1
export function addItem(_cart: Cart, _name: string, _cents: number): Cart {
  throw new Error("not implemented: addItem");
}
export function subtotal(_cart: Cart): number {
  throw new Error("not implemented: subtotal");
}

// T2
export function applyPercentOff(_cart: Cart, _pct: number): Cart {
  throw new Error("not implemented: applyPercentOff");
}

// T3
export function setTax(_cart: Cart, _pct: number): Cart {
  throw new Error("not implemented: setTax");
}
export function total(_cart: Cart): number {
  throw new Error("not implemented: total");
}

// T4
export function receipt(_cart: Cart): string {
  throw new Error("not implemented: receipt");
}
