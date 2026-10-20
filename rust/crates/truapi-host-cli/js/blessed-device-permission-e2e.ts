// A product's notification against a real signing-host CLI. The end-to-end
// check for paritytech/platform-bugs#82.
//
// A blessed product gets device permissions without a prompt. Any other
// product is asked once. `scripts/blessed-device-permission-e2e.sh` runs this
// as one product of each kind, with `--auto-accept` and `TRUAPI_APPROVALS_LOG`,
// so every prompt the host raised is a line in that file by the time the call
// that raised it resolves.
//
// Reading the result: the blessed run's transcript is empty and the other
// run's holds one device permission prompt. A blessed run that holds the
// prompt is platform-bugs#82.
import { existsSync, readFileSync } from "node:fs";

const DEVICE_PROMPT = "approved device permission";

const log = process.env.TRUAPI_APPROVALS_LOG;
assert(
  log,
  "TRUAPI_APPROVALS_LOG is not set; run scripts/blessed-device-permission-e2e.sh",
);
const blessed = process.env.E2E_BLESSED === "1";
const transcript = () =>
  existsSync(log)
    ? readFileSync(log, "utf8")
        .split("\n")
        .filter((line) => line.length > 0)
    : [];

const permission =
  await truapi.permissions.requestDevicePermission("Notifications");
assert(
  permission.isOk() && permission.value.granted,
  `Notifications was not granted: ${JSON.stringify(permission.isOk() ? permission.value : permission.error)}`,
);

const sent = await truapi.notifications.sendPushNotification({
  text: "platform-bugs#82",
});
assert(
  sent.isOk(),
  `notification failed: ${JSON.stringify(sent.isOk() ? null : sent.error)}`,
);

const prompts = transcript();
const expected = blessed ? [] : [DEVICE_PROMPT];
assert(
  JSON.stringify(prompts) === JSON.stringify(expected),
  `${host.productId}: expected prompts ${JSON.stringify(expected)}, got ${JSON.stringify(prompts)}`,
);
console.log(
  blessed
    ? `${host.productId} sent a notification without a device permission prompt`
    : `${host.productId} was asked once before sending a notification`,
);
