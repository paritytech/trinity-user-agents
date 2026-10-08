import { act, byId, formatUnits, start, status } from "./screen";

const DESTINATIONS = {
  Card: { label: "Card number", example: "4242 4242 4242 4242" },
  Bank: { label: "IBAN", example: "DE89 3704 0044 0532 0130 00" },
  Crypto: { label: "Polkadot Asset Hub address", example: "" },
} as const;

void start("/payout", (intent, view) => {
  const confirm = byId<HTMLButtonElement>("confirm");
  const input = byId<HTMLInputElement>("destination");
  const destination = DESTINATIONS[view?.rail ?? "Bank"];
  byId("destination-label").textContent = destination.label;
  input.value = destination.example;
  if (view?.amount !== undefined) {
    const paid =
      view.price === undefined
        ? ""
        : ` for ${formatUnits(view.price, view.decimals)} ${view.asset}`;
    byId("price").textContent = `${formatUnits(view.amount, 6)} CASH${paid}`;
  }
  confirm.disabled = false;
  confirm.addEventListener("click", async () => {
    const value = input.value.trim();
    if (!value) {
      status(`Enter a ${destination.label.toLowerCase()}.`, "error");
      return;
    }
    confirm.disabled = true;
    const handed = await act(intent, {
      kind: "payout-confirmed",
      destination: value,
    });
    if (handed) {
      status(
        "Confirmed. Approve the payment from your balance when asked; you can close this screen.",
        "done",
      );
    } else {
      confirm.disabled = false;
      status("The provider could not be told. Try again.", "error");
    }
  });
});
