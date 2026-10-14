# CI/CD System Reference for AI Agents

Complete technical reference for the Polkadot iOS CI/CD system. Use this as context when working with workflows, composite actions, Fastlane, or build scripts.

## Architecture Overview

```text
Workflows (orchestration)
  → Composite Actions (reusable step groups)
    → Python Scripts (file manipulation)
    → Fastlane Lanes (build / sign / distribute)
      → Xcode tooling (gym, scan, match)
```

Application secrets come from GitHub Actions repository secrets and are passed only to the steps that consume them. The `install` composite action does not fetch or expose application secrets.

---

## File Map

### Workflows (`.github/workflows/`)

| File | Trigger | Purpose |
|------|---------|---------|
| `pr.yml` | Any `pull_request`; jobs skip `release-*` branches and PRs with `skip-ci` label | Build + unit tests for regular PRs |
| `firebase_debug_distribution.yml` | `workflow_dispatch` or `pull_request.closed` on `develop` (merged only) | Build the DevCI variants and upload the default one to Firebase |
| `nightly_prepare.yml` | `workflow_dispatch` or weekday schedule at `16:00 UTC` | Prepare a nightly branch/PR (no version bump), trigger nightly distribution; skips when no changes vs `main` |
| `release_prepare.yml` | `workflow_dispatch` | Prepare a release branch/PR (optional version bump), trigger release distribution; `source_ref: main` dispatches a direct build with no branch/PR (`no-bump` only) |
| `_prepare_pipeline.yml` | `workflow_call` (reusable) | Shared prepare logic: bump, branch/PR creation, no-changes decision, distribution trigger |
| `nightly_distribution.yml` | bot `workflow_dispatch` | Nightly **and** Safetynet TestFlight builds from one branch/PR, internal TestFlight distribution, auto-merge PR once both succeed, combined Matrix notification |
| `release_distribution.yml` | `pull_request` (open/sync, nightly PRs excluded) to `main` and bot `workflow_dispatch` | Release TestFlight build, internal TestFlight distribution, Matrix notification |
| `_build_distribute.yml` | `workflow_call` (reusable) | Shared build/distribute: metadata, build, TestFlight upload, PR comment, result check |
| `release_branch_lifecycle.yml` | `pull_request.closed` to `main` (`release/*`) | Backport PR on merge; delete branch on close without merge (shared by both flows) |
| `testflight_distribution.yml` | `workflow_dispatch` | Ad-hoc TestFlight distribution for allowlisted actors |
| `update_signing_data.yml` | `workflow_dispatch` or daily schedule at `10:00 UTC` | Refresh signing assets through Fastlane Match |
| `collect_prs_summary.yml` | `workflow_dispatch` or monthly schedule | PR summary report |

### Composite Actions (`.github/actions/`)

| Action | Purpose | Key Detail |
|--------|---------|------------|
| `install/` | Setup iOS build environment | Validates and configures Match authentication from a GitHub PAT when requested, installs Xcode, Ruby and Python; does not load application secrets |
| `configure-google-services/` | Generate Firebase configuration | Decodes a Base64-encoded plist secret, validates it and its bundle ID, then writes the ignored `GoogleService-Info.plist` immediately before an Xcode build or test |
| `distribute-testflight/` | Run tests, build, upload to TestFlight | Caller must run `install/` first and provide `build_number`, App Store Connect credentials, signing passwords, and both Google service plist secrets; the action generates Dev config for tests and Release config for the archive |
| `read-build-version/` | Read Release `MARKETING_VERSION` + compute next TestFlight build number | Caller must run `install/` first and provide App Store Connect credentials; `increment_step` defaults to `1` |

### Python Scripts (`.github/scripts/`)

| Script | Purpose | Example |
|--------|---------|---------|
| `read_versions.py` | Read `MARKETING_VERSION` and `CURRENT_PROJECT_VERSION` from `project.pbxproj` | `python3 read_versions.py project.pbxproj --config-name Release --output-format env` |
| `update_build_number.py` | Set `CURRENT_PROJECT_VERSION` in `project.pbxproj` | `python3 update_build_number.py project.pbxproj --config-name DevCI --build-number 42` |
| `update_marketing_version.py` | Set `MARKETING_VERSION` in `project.pbxproj` | `python3 update_marketing_version.py project.pbxproj 1.2.3 --config-name Release` |
| `add_swift_flags.py` | Append extra flags to `OTHER_SWIFT_FLAGS` in an xcconfig | `python3 add_swift_flags.py polkadot-app.release.xcconfig -DDISABLE_AUTH` |

### Fastlane Lanes (`fastlane/`)

| Lane | File | Purpose |
|------|------|---------|
| `build_app_ci` | `Fastfile` | Build the app using `main_configuration`; consumes `BUILD_NUMBER` if provided |
| `distribute_app_to_firebase` | `Fastfile` | Build + upload to Firebase App Distribution |
| `distribute_testflight` | `Fastfile` | Build + upload to TestFlight; requires `BUILD_NUMBER` env var |
| `run_unit_tests` | `Fastfile` | Run unit tests |
| `base_build_app` | `fastlane/lanes/lane_build.rb` | Core build logic: plist copy, build number bump, signing, gym |
| `test_build` | `fastlane/lanes/lane_tests.rb` | Core unit test logic using `scan` |
| `distribute_to_firebase` | `fastlane/lanes/lane_firebase_distribute.rb` | Firebase upload step |
| `upload_testflight` | `fastlane/lanes/lane_testflight.rb` | TestFlight upload step |
| `load_asc_api_key` | `fastlane/lanes/lane_testflight.rb` | Load App Store Connect API credentials from env |
| `get_testflight_build_number` | `fastlane/lanes/lane_testflight.rb` | Query latest build number from TestFlight |
| `prepare_code_signing` | `fastlane/lanes/lane_signing.rb` | Fetch certs/profiles via Match in readonly mode |
| `update_signing_data` | `fastlane/lanes/lane_signing.rb` | Refresh Match-managed certs/profiles in write mode |

---

## Build Variants

Two workflows build matrix variants in parallel:
- `firebase_debug_distribution.yml`
- `_build_distribute.yml` (shared by nightly and release distribution)

Both use the same two variant shapes:

```yaml
variant:
  - name: "default"
    extra_swift_flags: ""
    artifact_suffix: ""
    upload_to_distribution: true
  - name: "no-auth"
    extra_swift_flags: "-DDISABLE_AUTH"
    artifact_suffix: "-no-auth"
    upload_to_distribution: false
```

Only `firebase_debug_distribution.yml` still builds two variants.
`_build_distribute.yml` builds one: its second leg had no destination once the
artifact uploads went, so it built and discarded.

Mapping to real workflow keys:
- Firebase workflow uses `upload_to_firebase`
- Release TestFlight workflow uses `upload_to_testflight`

Rules:
- Only one variant may upload to the external distribution service
- Extra Swift flags are injected into xcconfig via `add_swift_flags.py`, into the leaf
  config named after `build_configuration`
- `no-auth` artifacts use the `-no-auth` suffix in the artifact name
- Release TestFlight flow explicitly shares one build number across both variants via the `prepare_build_metadata` job
- Firebase flow produces the same build number for both variants because both derive it from the same `github.run_number`

---

## Build Number Management

### PR validation builds
- Source: `${{ github.run_number }}`
- Defined in `pr.yml`

### Firebase builds (development)
- Source: `10100 + github.run_number`
- Written to `project.pbxproj` via `update_build_number.py` before building

### Release TestFlight builds
- Source of truth: App Store Connect / TestFlight API
- Calculation: `latest_testflight_build_number + increment_step`
- `increment_step` comes from release PR metadata:
  `<!-- RELEASE_METADATA: increment_step=N -->`
  (direct builds from `main` have no PR — it is passed as a workflow input instead)
- `_build_distribute.yml` reads the number once in `prepare_build_metadata` (via `read-build-version`)
- The resulting `build_number` is passed to the build job
- Build number is not committed to the repository

### Manual TestFlight builds
- Source of truth: App Store Connect / TestFlight API
- Calculation: `latest_testflight_build_number + 1`
- Computed in `testflight_distribution.yml` and passed to the composite action

---

## Xcode Configurations

| Config | xcconfig | Used By |
|--------|----------|---------|
| `DevCI` | `polkadot-app/Configs/polkadot-app.devci.xcconfig` | PR validation, Firebase distribution |
| `Release` | `polkadot-app/Configs/polkadot-app.release.xcconfig` | Release and manual TestFlight distribution |
| `Debug` | `polkadot-app/Configs/polkadot-app.debug.xcconfig` | Local development |

Fastlane configuration selection:
- `FASTLANE_CONFIGURATION` controls `main_configuration` and defaults to `DevCI`
- `distribute_testflight` always builds with `Release`

---

## Bundle IDs and Signing

| Environment | Bundle ID | Extension Bundle ID | Signing |
|-------------|-----------|---------------------|---------|
| Development | `io.parity.polkadotapp.develop` | `io.parity.polkadotapp.develop.NotificationServiceExtension` | `match Development` or `match AdHoc` depending on workflow |
| Safetynet | `io.parity.polkadotapp.safety` | `io.parity.polkadotapp.safety.NotificationServiceExtension` | `match AppStore` |
| Production | `io.parity.polkadotapp` | `io.parity.polkadotapp.NotificationServiceExtension` | `match AppStore` |

`Safetynet` compiles the Release feature set with the environment flags
`-DNIGHTLY -DTESTNET_FEATURE`, so it is what Release ships, run against the nightly chains —
under its own identity so it installs next to the production app. `nightly_distribution.yml`
builds it alongside `Nightly`.

Safetynet is a **separate app**, not a variant of production: its own App Store Connect
record, TestFlight build-number sequence, Match profiles and Firebase app. `_build_distribute.yml`
derives all four identifiers from `inputs.build_configuration`; `read-build-version` is passed the
matching `ios_bundle_id` so the two streams never share a build counter. Nightly carries the
production identity and differs from Release only in flags and app icon.

Signing notes:
- `pr.yml` uses `development`
- `firebase_debug_distribution.yml` uses `ad-hoc`
- TestFlight workflows use `app-store`
- All signing types use `paritytech/fastlane-polkadotapp-develop`
- `prepare_code_signing` fetches both main app and extension profiles when the extension bundle ID is set

---


## Release Flow

```text
1. nightly_prepare.yml / release_prepare.yml
   └── uses _prepare_pipeline.yml
       ├── Validate actor for manual runs (scheduled runs skip)
       ├── Read current Release MARKETING_VERSION; optionally bump (release flow)
       ├── Decide should_release = has_changes || !is_nightly
       │     ├── nightly with no changes -> stop (nothing created)
       │     └── release with source_ref=main (no-bump only) -> direct build:
       │         skip branch/PR steps, dispatch distribution against main with no PR
       ├── Create branch: release-X.Y[.Z]-YYYYMMDDhhmm
       │     └── release flow with no changes -> empty placeholder commit
       ├── Collect PR summary / release notes
       ├── Create PR: release-X.Y[.Z]-YYYYMMDDhhmm -> main
       └── Trigger nightly_distribution.yml / release_distribution.yml via workflow_dispatch

2. nightly_distribution.yml / release_distribution.yml
   └── uses _build_distribute.yml
       (nightly calls it twice — Nightly and Safetynet from the same branch/PR;
        the configuration is in the job name so each leg's check_build_result
        resolves its own result)
       ├── prepare_build_metadata
       │     ├── Verify bot-driven trigger / bot-authored PR
       │     ├── Read release notes + increment_step from PR body
       │     │     └── no PR (direct build) -> generic notes + increment_step input
       │     ├── Setup iOS environment
       │     └── read-build-version: MARKETING_VERSION + shared build_number
       ├── check_and_build
       │     ├── distribute-testflight (tests + build + upload)
       │     ├── comment on PR with build info
       │     └── set commit status for workflow_dispatch
       ├── check_build_result -> succeeded output
       └── send_failure_notification (Telegram, on failure)

3. Caller-specific jobs (gated on succeeded)
   ├── [nightly] auto_merge PR + delete branch (needs BOTH legs green)
   └── send Matrix release notification
         ├── [nightly] one combined message covering both legs, all four links
         └── [release] its own message, release links only

4. release_branch_lifecycle.yml (on release/* PR closed to main)
   ├── merged -> create backport PR: release branch -> source_ref
   │     └── skipped when source_ref equals the PR base branch
   └── closed without merge -> delete release branch
```

---

## Manual TestFlight Flow

```text
1. testflight_distribution.yml
   ├── Restrict workflow to allowlisted GitHub actors
   ├── Read Release MARKETING_VERSION
   ├── Setup iOS environment
   ├── Query TestFlight and calculate build_number = latest + 1
   ├── Run distribute-testflight action (tests + build + upload)
   └── Upload IPA artifact
```

---

## Key Environment Variables

| Variable | Where Set | Purpose |
|----------|-----------|---------|
| `BUILD_NUMBER` | Workflow or composite action env | Build number passed into Fastlane |
| `FASTLANE_CONFIGURATION` | Workflow env | Select Xcode build configuration for `build_app_ci` |
| `RUN_IN_CI` | Workflow env | CI mode flag used by Fastlane/Xcode |
| `IOS_BUNDLE_ID` | Workflow env | Main app bundle identifier |
| `IOS_EXTENSION_BUNDLE_ID` | Workflow env | Notification service extension bundle identifier |
| `PROVISIONING_PROFILE_SPECIFIER` | Workflow env | Main app profile name for code signing |
| `EXTENSION_PROVISIONING_PROFILE_SPECIFIER` | Workflow env | Extension profile name for code signing |
| `EXPORT_METHOD` | Workflow env | `development`, `ad-hoc`, or `app-store` |
| `ASC_KEY_ID`, `ASC_ISSUER_ID`, `ASC_KEY_BASE64` | GitHub Actions repository secrets -> Apple-facing workflows/actions | Team App Store Connect API auth for TestFlight and writable Match |
| `FIREBASE_APP_ID` | GitHub Actions repository secret -> Firebase distribution step | Firebase distribution target |
| `CREDENTIAL_FILE_CONTENT` | GitHub Actions repository secret -> Firebase workflow | Firebase service-account JSON for App Distribution |
| `GOOGLE_SERVICE_INFO_DEV_BASE64` | GitHub Actions repository secret -> DevCI build/test setup steps | Base64-encoded development `GoogleService-Info.plist` for `io.parity.polkadotapp.develop` |
| `GOOGLE_SERVICE_INFO_RELEASE_BASE64` | GitHub Actions repository secret -> Release/Nightly build setup steps | Base64-encoded production `GoogleService-Info.plist` for `io.parity.polkadotapp` |
| `GOOGLE_SERVICE_INFO_SAFETY_BASE64` | GitHub Actions repository secret -> Safetynet build setup steps | Base64-encoded `GoogleService-Info.plist` for `io.parity.polkadotapp.safety` |
| `FASTLANE_RO_PAT` | GitHub Actions repository secret -> `install/` | Read-only fine-grained PAT for Match in PR, Firebase, and TestFlight builds |
| `FASTLANE_RW_PAT` | GitHub Actions repository secret -> `install/` | Read/write fine-grained PAT used only by signing-data refresh |
| `MATCH_GIT_BASIC_AUTHORIZATION` | Generated by `install/` | Base64 Basic authorization derived from the selected Match repository PAT |
| `MATCH_PASSWORD` | GitHub Actions repository secret -> signing/build steps | Match decryption password |
| `KEYCHAIN_PASSWORD` | GitHub Actions repository secret -> signing/build steps | Temporary CI keychain password |
| `RELEASE_NOTES` | Workflow -> Fastlane | Release notes for Firebase / TestFlight |
| `DISTRIBUTE_EXTERNAL` | Workflow -> Fastlane | Whether to distribute TestFlight build externally |
| `EXTERNAL_GROUP_NAME` | Workflow -> Fastlane | Target external TestFlight group name |
| `FIREBASE_GROUPS` | Workflow env | Firebase distribution groups |
| `DELIVER_ALTOOL_ADDITIONAL_UPLOAD_PARAMETERS` | Manual TestFlight workflow env | Legacy altool upload flag |

---

## Security

- Application secrets are stored as GitHub Actions repository secrets and passed only to their consuming steps; do not expose them at job or workflow scope
- Google service plist files are not tracked; CI generates `polkadot-app/GoogleService-Info.plist` from the environment-specific Base64 secret after checkout and immediately before the consuming build/test step
- The prepare flow (`_prepare_pipeline.yml`, used by `nightly_prepare.yml` and `release_prepare.yml`) restricts manual dispatch to an allowlist of GitHub actors; scheduled nightly runs skip the check
- `testflight_distribution.yml` is restricted to its own actor allowlist
- The distribution flow (`_build_distribute.yml`, used by `nightly_distribution.yml` and `release_distribution.yml`) does not use an actor allowlist; instead:
  - `workflow_dispatch` must be triggered by `github-actions[bot]`
  - release PRs must be authored by `github-actions[bot]`
- Code signing uses Match in readonly mode everywhere except `update_signing_data.yml`
- CI creates a temporary keychain with a 1-hour timeout

---

## Practical Notes for Agents

- Nightly and release share reusable workflows: `_prepare_pipeline.yml` (prepare) and `_build_distribute.yml` (build/distribute). The build mode (`Nightly`/`Release`, external group) is passed by the caller as inputs — do not reintroduce parsing it from PR metadata
- Reusable workflows declare the secrets they read, and every caller passes exactly those; the prepare callers also need an explicit `permissions:` write block (env context is unavailable in a reusable-workflow `with:` block)
- When editing the build flow, remember that shared build metadata lives in `prepare_build_metadata`
- When touching `distribute-testflight`, verify both callers still pass `build_number`
- Every CI Xcode build/test entry point must run `configure-google-services` after its final checkout; the TestFlight composite action owns both its Dev test config and Release archive config
- When reviewing release logic, treat PR metadata in the release PR body as part of the contract:
  - `source_ref`
  - `nightly_build`
  - `increment_step`
