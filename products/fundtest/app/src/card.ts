import { act, byId, formatUnits, parseCash, start, status } from "./screen";

void start("/card", (intent, view) => {
  const pay = byId<HTMLButtonElement>("pay");
  const amountField = byId("amount-field");
  const amountInput = byId<HTMLInputElement>("amount");
  if (view?.amount !== undefined) {
    const price =
      view.price === undefined
        ? ""
        : ` for ${formatUnits(view.price, view.decimals)} ${view.asset}`;
    byId("price").textContent = `${formatUnits(view.amount, 6)} CASH${price}`;
  } else {
    byId("price").textContent = "Choose an amount";
    amountField.hidden = false;
  }
  pay.disabled = false;
  pay.addEventListener("click", async () => {
    const amount =
      view?.amount === undefined ? parseCash(amountInput.value) : undefined;
    if (view?.amount === undefined && amount === undefined) {
      status("Enter an amount of CASH, such as 25.00.", "error");
      return;
    }
    pay.disabled = true;
    status("Paying…");
    const sent = await act(intent, {
      kind: "card-paid",
      ...(amount === undefined ? {} : { amount }),
    });
    if (sent) {
      status(
        "Payment sent. You can close this screen; the funds follow in a few minutes.",
        "done",
      );
    } else {
      pay.disabled = false;
      status(
        "The payment could not be handed to the provider. Try again.",
        "error",
      );
    }
  });
});
