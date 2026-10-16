// `preimage.submit` under a `PreimageSubmit` grant against a real signing-host CLI.
// The remote permission authorizes the capability; each upload still requires a
// size review. The runner uses `--auto-accept` and `TRUAPI_APPROVALS_LOG` so the
// transcript contains one permission prompt followed by one review per upload.
import { existsSync, readFileSync } from "node:fs";

const UPLOADS = 3;
const PERMISSION_PROMPT = "approved remote permission";
const UPLOAD_PROMPT = "approved submit preimage";

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
  prompts.length === UPLOADS + 1 &&
    prompts[0] === PERMISSION_PROMPT &&
    prompts.slice(1).every((prompt) => prompt === UPLOAD_PROMPT),
  `expected one permission prompt and ${UPLOADS} upload reviews, got: ${JSON.stringify(prompts)}`,
);
console.log(
  `${UPLOADS} uploads after one permission prompt and ${UPLOADS} upload reviews`,
);
