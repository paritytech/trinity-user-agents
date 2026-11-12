// `preimage.submit` under a `PreimageSubmit` grant, against a real
// signing-host CLI. The end-to-end check for paritytech/platform-bugs#54.
//
// The permission is the only gate on an upload: an "Allow always" grant
// covers every upload that follows, with no per-upload confirmation. Before
// the fix the host asked the user to approve each upload as well, so a
// background backup prompted on every run.
//
// `scripts/preimage-submit-prompt-e2e.sh` runs the host with `--auto-accept`
// and `TRUAPI_APPROVALS_LOG`, so every prompt the host raised is a line in that
// file by the time the call that raised it resolves.
//
// Reading the result: the transcript holds the one permission prompt and
// nothing else. Before the fix it also held one `submit preimage` line per
// upload.
import { existsSync, readFileSync } from "node:fs";

const UPLOADS = 3;
const PERMISSION_PROMPT = "approved remote permission";

const log = process.env.TRUAPI_APPROVALS_LOG;
assert(
  log,
  "TRUAPI_APPROVALS_LOG is not set; run scripts/preimage-submit-prompt-e2e.sh",
);
const transcript = () =>
  existsSync(log)
    ? readFileSync(log, "utf8")
        .split("\n")
        .filter((line) => line.length > 0)
    : [];

const permission = await truapi.permissions.requestRemotePermission({
  permission: { tag: "PreimageSubmit" },
});
assert(
  permission.isOk() && permission.value.granted,
  "PreimageSubmit was not granted",
);

for (let upload = 1; upload <= UPLOADS; upload++) {
  const value =
    `0x${crypto.getRandomValues(new Uint8Array(1024)).toHex()}` as `0x${string}`;
  const submitted = await truapi.preimage.submit(value);
  assert(
    submitted.isOk(),
    `upload ${upload} failed: ${JSON.stringify(submitted.isOk() ? null : submitted.error)}`,
  );
  console.log(`upload ${upload}: ${submitted.value}`);
}

const prompts = transcript();
assert(
  prompts.length === 1 && prompts[0] === PERMISSION_PROMPT,
  `expected only the permission prompt across ${UPLOADS} uploads, got: ${JSON.stringify(prompts)}`,
);
console.log(
  `${UPLOADS} uploads after one permission prompt, no per-upload confirmation`,
);
