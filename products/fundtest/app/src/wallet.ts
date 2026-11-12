import { act, allowFaucet, byId, formatUnits, start, status } from "./screen";

const NETWORK_NAMES: Record<string, string> = {
  polkadot: "Polkadot Asset Hub",
};

void start("/wallet", (intent, view) => {
  const send = byId<HTMLButtonElement>("send");
  const crypto = view?.crypto;
  if (!crypto) {
    status(
      "The deposit details are not ready yet. Reopen this screen.",
      "error",
    );
    return;
  }
  byId("price").textContent =
    `${formatUnits(crypto.amount, crypto.decimals)} ${crypto.asset}`;
  byId("address").textContent = crypto.address;
  byId("network").textContent = NETWORK_NAMES[crypto.network] ?? crypto.network;
  if (crypto.demo) {
    byId("note").textContent =
      "Test provider. Send stands in for a wallet: the provider deposits test PAS from the public Paseo faucet. A real transfer to the address counts too.";
  } else {
    send.textContent = "I've sent it";
  }
  send.disabled = false;
  send.addEventListener("click", async () => {
    send.disabled = true;
    if (crypto.demo && !(await allowFaucet())) {
      send.disabled = false;
      status(
        "The test deposit is drawn from the public Paseo faucet. Allow network access to it to continue.",
        "error",
      );
      return;
    }
    status("Sending…");
    if (await act(intent, { kind: "wallet-sent" })) {
      status("Sent. The deposit is confirmed on chain in a minute.", "done");
    } else {
      send.disabled = false;
      status("The provider could not be told. Try again.", "error");
    }
  });
});
