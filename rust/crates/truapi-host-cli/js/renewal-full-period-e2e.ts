// Allocates a statement-store allowance for the product it runs as, against a
// real signing-host CLI. `scripts/renewal-full-period-e2e.sh` runs it once per
// product to set up renewal targets before it fills the period.
//
// Allocation registers the product's allowance account in a free slot and
// records it in the signing host's renewal ledger, so the next renewal pass
// treats it as a target.
const login = await truapi.account.requestLogin({ reason: undefined });
assert(
  login.isOk() &&
    (login.value === "Success" || login.value === "AlreadyConnected"),
  "requestLogin failed:",
  login,
);

const result = await truapi.resourceAllocation.request({
  resources: [{ tag: "StatementStoreAllowance" }],
});
assert(result.isOk(), "resource allocation failed:", result);
assert(
  result.value.outcomes.length === 1 &&
    result.value.outcomes[0] === "Allocated",
  "expected the statement-store allowance to be allocated:",
  result.value,
);
console.log("statement-store allowance allocated");

export {};
