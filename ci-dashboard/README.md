# CI dashboard

One page for the nightly builds, the scheduled workflows and the services CI depends on (#1662). It runs as a small
server: one process holds the GitHub token, the relay token and the webhook secrets, runs the checks on a schedule, and
serves the page. Viewers need no token and make no GitHub API calls. It is deployed as a product on Parity's clusters,
next to the host team's Matrix relay, and posts through that relay.

The page shows:

- **Downloads:** the latest Android APKs with an Add to Obtainium button, the iOS simulator build with its install
  commands, and the CLI install command.
- **Nightlies:** the newest Android and iOS (simulator) prereleases, with their downloads. The newest Android nightly
  shows whether its host-playground e2e jobs passed.
- **Releases:** the latest version of each package and the recent release history.
- **Workflows:** the latest run of each workflow in `config.json`, and the result of the daily smoke checks.
- **Nodes:** `system_health` and the latest block from each RPC endpoint.
- **Uptime:** each published link, and the TLS certificate of each host.
- **Sentry:** the projects whose new issues are posted to Matrix.

## What the server does

| When                | What                                                                                                                                                                                                                                     |
| ------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| GitHub webhook      | A failed run of a watched workflow, or of Release, is posted to Matrix. A published release that is not a prerelease is announced; nightlies announce themselves. Either refreshes the page's GitHub data.                               |
| Sentry webhook      | A new issue in one of the projects in `config.json` is posted to Matrix.                                                                                                                                                                 |
| Every minute        | Each RPC endpoint and published link is checked. One that fails two checks in a row is posted, and again when it recovers.                                                                                                               |
| Every hour          | Each host's TLS certificate is read. Less than two weeks left counts as failing, and is posted the same way.                                                                                                                             |
| Every 5 minutes     | GitHub data is read again, in case a webhook was missed.                                                                                                                                                                                 |
| Each day, 06:37 UTC | `smoke.sh` runs after the nightlies: the CLI install command from the docs installs and runs the stable version, each npm package installs and imports, and the fixed Android nightly link serves the newest build. A failure is posted. |

Check state is kept in `STATE_DIR`, so a restart neither forgets an outage nor posts it twice. Posts go out one at a
time, and what arrives while one is being sent, such as a burst of Sentry issues, is folded into the next. Without the
relay settings the server logs what it would have posted.

`smoke.sh` installs and runs what it downloads, so it runs with only `PATH` and `HOME` and no secret, installs npm
packages with `--ignore-scripts`, and is stopped after 15 minutes.

## Changing what is watched

Edit `config.json`: `workflows` for the Workflows view and its failure alerts, `alertOnFailure` for workflows that alert
but are not shown (Release skips every commit that is not a release), `rpcEndpoints`, `links` and `sentry.projects`.
Each file in `workflows` must exist under `.github/workflows/`; a test checks it.

A workflow entry can narrow what it reads. `title` keeps only runs with that title, so `iOS Nightly (main)` leaves out
branch nights. `job` reads only the jobs whose names start with it, for a leg such as the simulator bundle that runs
inside the iOS nightly's run. A nightly's `e2eJob` does the same for the e2e jobs of the run its release notes link.

## Running it locally

```bash
bun test ci-dashboard/test
bun ci-dashboard/build.ts ci-dashboard/dist
bun ci-dashboard/server/main.ts      # then open http://localhost:8787
```

With no environment set it reads GitHub anonymously, posts nothing, and refuses webhooks. The image is built from
`ci-dashboard/` as its context: `docker build -t ci-dashboard ci-dashboard`, then run it with `-p 8080:8080`.
`ci-dashboard/smoke.sh` runs the smoke checks on their own; it needs curl, jq, node, npm and `aapt` from the Android
SDK.

## Deployment

The dashboard is a product on Parity's clusters, deployed the way `paritytech/ci-host-matrix-relay` is: by the
`products` ApplicationSet in `paritytech/devops-cloud-infra`, which reads three files at this repository's root.

| File                       | What it is                                                                                         |
| -------------------------- | -------------------------------------------------------------------------------------------------- |
| `manifest.yaml`            | Registry, Dockerfile, port, health checks on `/up`, resources, and a 1 GB volume for the state     |
| `manifest-staging.yaml`    | parity-stg, internal only. It has no relay token, so it logs alerts rather than posting them twice |
| `manifest-production.yaml` | parity-prod, public for the webhooks, `ci-host-dashboard.parity-prod.parity.io`                    |

The CI dashboard job in `ci.yml` tests each change and builds the image, without pushing it. Pushing to Artifact
Registry comes with going live, as its own workflow once devops has set up the registry; Kargo then discovers each image
and promotes it through staging to production. Secrets are `ref+vault://` references under
`kv/argo-cd/ci-host-dashboard/`, resolved at sync time, so none is in git.

| Variable                 | Where                      | Notes                                                                                            |
| ------------------------ | -------------------------- | ------------------------------------------------------------------------------------------------ |
| `GITHUB_TOKEN`           | Vault                      | A fine-grained token with no permissions: the repository is public, and it raises the API limit. |
| `NOTIFICATION_BOT_TOKEN` | Vault                      | The relay's API token, the same value as the workflows' `NOTIFICATION_BOT_TOKEN` secret.         |
| `GITHUB_WEBHOOK_SECRET`  | Vault                      | The repository webhook's secret.                                                                 |
| `SENTRY_CLIENT_SECRET`   | Vault                      | The Sentry internal integration's client secret.                                                 |
| `NOTIFICATION_BOT_URL`   | `manifest-production.yaml` | The relay, `https://ci-host-matrix-relay.paritytech.io`.                                         |
| `MATRIX_ROOM_IDS`        | `manifest-production.yaml` | The alerts room. It must be unencrypted and on the relay's `MATRIX_ALLOWED_ROOMS`.               |
| `DASHBOARD_URL`          | both manifests             | Linked from alerts.                                                                              |

The container starts as root only to hand the volume at `/data` to the `node` user, then runs the server as that user.

### Webhooks

- **GitHub:** a repository webhook to `https://ci-host-dashboard.parity-prod.parity.io/webhooks/github`, content type
  `application/json`, with `GITHUB_WEBHOOK_SECRET` as its secret, for **Workflow runs** and **Releases**.
- **Sentry:** an internal integration with its webhook URL set to
  `https://ci-host-dashboard.parity-prod.parity.io/webhooks/sentry` and the **issue** webhook on. Its client secret is
  `SENTRY_CLIENT_SECRET`. Also set `sentry.organization` in `config.json`.

### Android nightlies in Obtainium

The **Add to Obtainium** button on the Downloads view opens Obtainium with the repository filled in. Turn on **Include
prereleases**, and under **APK selection** set **Filter links by regular expression** to `app-gp-nightly\.apk`.
Obtainium then installs every new nightly. Checked with Obtainium 1.7.1 on an Android 15 emulator.
