/**
 * A product App for the browser test of preimage images: it looks a preimage
 * up with TrUAPI, as Browse looks up an app's icon, and draws the bytes as an
 * image. It reaches the host only through TrUAPI.
 */
import { getClientSync } from "@parity/truapi/sandbox";

const found = getClientSync();
if (found === null) throw new Error("no TrUAPI host");
const client = found;

function bytesOf(hex: string): Uint8Array<ArrayBuffer> {
  const bytes = new Uint8Array(new ArrayBuffer((hex.length - 2) / 2));
  for (let index = 0; index < bytes.length; index += 1)
    bytes[index] = Number.parseInt(hex.slice(2 + index * 2, 4 + index * 2), 16);
  return bytes;
}

(
  window as unknown as {
    __icon: { show(key: `0x${string}`): Promise<unknown> };
  }
).__icon = {
  show(key) {
    return new Promise((resolve) => {
      const lookup = client.preimage
        .lookupSubscribe({ request: { key } })
        .subscribe({
          next({ value }) {
            if (value === undefined) return;
            lookup.unsubscribe();
            const image = document.createElement("img");
            image.onload = () =>
              resolve({
                width: image.naturalWidth,
                height: image.naturalHeight,
              });
            image.onerror = () =>
              resolve({ error: "the bytes are not an image" });
            image.src = URL.createObjectURL(new Blob([bytesOf(value)]));
            document.body.append(image);
          },
          error: (reason) => resolve({ error: String(reason) }),
          complete: () => resolve({ error: "the lookup ended" }),
        });
    });
  },
};
