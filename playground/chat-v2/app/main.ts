/**
 * The App of the sample product: it opens the two rooms through
 * `system.navigate_to` and shows the command table. It calls no `Chat`
 * method, because `Chat` is a Worker service.
 */
import { getClientSync } from "@parity/truapi/sandbox";
import type { RoomId } from "../worker/clock";
import { COMMANDS } from "../worker/commands";
import { roomLink } from "../worker/links";

function element<T extends HTMLElement>(id: string): T {
  const found = document.getElementById(id);
  if (!found) throw new Error(`the page has no #${id}`);
  return found as T;
}

function showStatus(text: string): void {
  element("status").textContent = text;
}

async function openRoom(room: RoomId): Promise<void> {
  const client = getClientSync();
  if (!client) {
    showStatus("No host connection: open this page inside a TrUAPI host.");
    return;
  }
  const url = roomLink(room);
  const result = await client.system.navigateTo({ url });
  showStatus(result.isOk() ? `Opened ${url}` : `navigate_to ${url} failed: ${JSON.stringify(result.error)}`);
}

function renderCommands(): void {
  const body = element<HTMLTableSectionElement>("commands");
  for (const command of COMMANDS) {
    const row = document.createElement("tr");
    const cells = [`/${command.name}`, command.args, command.description, command.rooms, command.scenarios];
    cells.forEach((text, index) => {
      const cell = document.createElement("td");
      if (index < 2 && text !== "") {
        const code = document.createElement("code");
        code.textContent = text;
        cell.append(code);
      } else {
        cell.textContent = text;
      }
      row.append(cell);
    });
    body.append(row);
  }
}

if (new URLSearchParams(window.location.search).get("from") === "deeplink") {
  element("banner").classList.add("shown");
}
element("open-host").addEventListener("click", () => void openRoom("host"));
element("open-source").addEventListener("click", () => void openRoom("source"));
renderCommands();
