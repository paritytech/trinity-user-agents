// The product page shown under the loyalty card's face when the card is opened. Every call's
// answer and the page height are on screen, and a bar pinned to the bottom edge disappears when
// the host sizes the page wrong, so a screen recording shows what the host did.
import { getClientSync } from "@parity/truapi/sandbox";

const client = getClientSync();
if (!client) {
  throw new Error("The expanded card page needs a TrUAPI host connection");
}

const COVER_DELAY_MS = 2000;

const log = document.getElementById("log") as HTMLUListElement;
const height = document.getElementById("height") as HTMLSpanElement;

function logLine(message: string): void {
  const line = document.createElement("li");
  line.textContent = `${new Date().toLocaleTimeString()} ${message}`;
  log.prepend(line);
}

const setCardCovered = async (covered: boolean): Promise<void> => {
  const outcome = await client.expandedCard.setCardCovered({ covered });
  const answer = outcome.isOk() ? "ok" : errorTag(outcome.error);
  logLine(`setCardCovered(${covered}): ${answer}`);
};

// A domain error nests the versioned error one level down; the other kinds are the tag itself.
function errorTag(error: { tag: string; value?: unknown }): string {
  if (error.tag !== "Domain") return error.tag;
  const versioned = error.value as { value: { tag: string } };
  return versioned.value.tag;
}

function showHeight(): void {
  height.textContent = String(window.innerHeight);
}

document.getElementById("cover")!.addEventListener("click", () => void setCardCovered(true));
document.getElementById("uncover")!.addEventListener("click", () => void setCardCovered(false));
document.getElementById("cover-later")!.addEventListener("click", () => {
  logLine(`covering in ${COVER_DELAY_MS / 1000} s`);
  setTimeout(() => void setCardCovered(true), COVER_DELAY_MS);
});
window.addEventListener("resize", () => {
  showHeight();
});

showHeight();
if (new URLSearchParams(location.search).has("coverOnLoad")) {
  void setCardCovered(true);
}
