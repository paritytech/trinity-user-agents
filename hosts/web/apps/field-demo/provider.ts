import { connectFields, element, FieldError } from "./client.js";
import { createToast, fillIcons } from "./ui.js";
import type { Field, ProviderStatus } from "../../bridges/fields/protocol.js";

/**
 * The name this app gives itself, set per server by `VITE_FIELD_PROVIDER_NAME`
 * so two instances read apart. The host shows it beside the app's verified id.
 */
const NAME: string =
  import.meta.env.VITE_FIELD_PROVIDER_NAME ?? "Field notebook";
const FIELDS: Field[] = ["address", "phone"];

fillIcons();
document.title = NAME;
element("brand-name").textContent = NAME;
const toast = createToast(element("toast"));
const save = element<HTMLButtonElement>("save");
const form = element<HTMLFormElement>("values");
const saved = element("saved");
const address = element<HTMLTextAreaElement>("address");
const phone = element<HTMLInputElement>("phone");

/** The values this page typed, or null after a reload: then only the host holds them. */
type Shown = { address: string; phone: string } | null;

/** Show the form, or the summary of what this app gave the host. */
function showSaved(fields: Field[], values: Shown): void {
  const any = fields.length > 0;
  form.hidden = any;
  saved.hidden = !any;
  for (const name of FIELDS) {
    const chip = element(`state-${name}`);
    const isSaved = fields.includes(name);
    chip.textContent = isSaved ? "Saved" : "Not set";
    if (isSaved) chip.dataset.tone = "success";
    else delete chip.dataset.tone;
  }
  if (!any) return;
  const kept = "Kept in your host. Edit to replace it.";
  element("saved-address").textContent = values?.address ?? kept;
  element("saved-phone").textContent = values?.phone ?? kept;
}

try {
  const call = await connectFields("provider");
  const status = (await call({ op: "status" })) as ProviderStatus;
  showSaved(status.saved, null);
  let registered = status.registered;
  save.disabled = false;
  element("edit").onclick = () => {
    form.hidden = false;
    saved.hidden = true;
    address.focus();
  };
  form.onsubmit = async (event) => {
    event.preventDefault();
    save.disabled = true;
    save.textContent = "Saving…";
    try {
      if (!registered) {
        await call({ op: "declare", name: NAME, fields: FIELDS });
        registered = true;
      }
      const values = { address: address.value, phone: phone.value };
      await call({ op: "save", values });
      showSaved(FIELDS, values);
      toast("Saved. Shops can now ask to ship to you.", "success");
    } catch (error) {
      const text = error instanceof Error ? error.message : "Not saved.";
      toast(
        text,
        error instanceof FieldError && error.code === "cancelled"
          ? "info"
          : "error",
      );
    } finally {
      save.disabled = false;
      save.textContent = "Save to my Polkadot host";
    }
  };
} catch (error) {
  toast(error instanceof Error ? error.message : "Not connected.", "error");
}
