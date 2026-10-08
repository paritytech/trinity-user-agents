import { act, byId, formatUnits, parseCash, start, status } from "./screen";

void start("/bank", (intent, view) => {
  const sent = byId<HTMLButtonElement>("sent");
  const amountInput = byId<HTMLInputElement>("amount");
  if (view?.amount !== undefined && view.price !== undefined) {
    byId("price").textContent =
      `${formatUnits(view.price, view.decimals)} ${view.asset}`;
  } else {
    byId("price").textContent = "Choose an amount";
    byId("amount-field").hidden = false;
  }
  if (view?.bank) {
    byId("beneficiary").textContent = view.bank.beneficiary;
    byId("account").textContent = view.bank.account;
    byId("bank-code").textContent = view.bank.bankCode;
    byId("reference").textContent = view.bank.reference;
  }
  sent.disabled = false;
  sent.addEventListener("click", async () => {
    const amount =
      view?.amount === undefined ? parseCash(amountInput.value) : undefined;
    if (view?.amount === undefined && amount === undefined) {
      status("Enter an amount of CASH, such as 25.00.", "error");
      return;
    }
    sent.disabled = true;
    const handed = await act(intent, {
      kind: "bank-sent",
      ...(amount === undefined ? {} : { amount }),
    });
    if (handed) {
      status(
        "Thanks. The transfer is matched by its reference; you can close this screen.",
        "done",
      );
    } else {
      sent.disabled = false;
      status("The provider could not be told. Try again.", "error");
    }
  });
});
