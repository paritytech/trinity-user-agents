// The product page shown under the loyalty card's face when the card is opened. Every call's
// answer and the page height are on screen, and a bar pinned to the bottom edge disappears when
// the host sizes the page wrong, so a screen recording shows what the host did.
import { getClientSync } from "@parity/truapi/sandbox";

const client = getClientSync();
if (!client) {
  throw new Error("The expanded card page needs a TrUAPI host connection");
}

const HIDE_DELAY_MS = 2000;

const log = document.getElementById("log") as HTMLUListElement;
const height = document.getElementById("height") as HTMLSpanElement;

function logLine(message: string): void {
  const line = document.createElement("li");
  line.textContent = `${new Date().toLocaleTimeString()} ${message}`;
  log.prepend(line);
}

const setFaceShown = async (shown: boolean): Promise<void> => {
  const outcome = await client.expandedCard.setFaceShown({ shown });
  const answer = outcome.isOk() ? "ok" : errorTag(outcome.error);
  logLine(`setFaceShown(${shown}): ${answer}`);
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

document.getElementById("hide")!.addEventListener("click", () => void setFaceShown(false));
document.getElementById("show")!.addEventListener("click", () => void setFaceShown(true));
document.getElementById("hide-later")!.addEventListener("click", () => {
  logLine(`hiding in ${HIDE_DELAY_MS / 1000} s`);
  setTimeout(() => void setFaceShown(false), HIDE_DELAY_MS);
});
window.addEventListener("resize", () => {
  showHeight();
});

showHeight();
if (new URLSearchParams(location.search).has("hideOnLoad")) {
  void setFaceShown(false);
}
