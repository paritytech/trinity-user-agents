# Device Authentication and Confirmation Gates

## Overview

Device authentication asks the person holding the phone for Face ID, Touch ID or the device passcode. The app uses it at each control that confirms a transfer, a signing, a device pairing, a product prompt or a manual backup.

Why: an unlocked phone must not be enough to sign, pay, pair or export a secret.

The gate sits at the confirm control, not at key access. Keys stay readable without device authentication.

## Confirmation Gates

### Rule

On a confirm tap, the app shows the device authentication prompt before the action runs.

- **Success:** the action runs as before.
- **Cancel or failure:** nothing runs. The screen stays in its pre-tap state. There is no new error UI.
- **No passcode:** the existing "authentication failed" sheet shows. The action does not run.
- **Every time:** each gated action prompts. A past device authentication never counts toward a later gate.
- **Prompt already live:** a request made while a prompt is live completes with `false` and runs nothing.

### Gated Actions

| Action | Gate location |
|--------|---------------|
| Transfer submit, after validation and the privacy confirmation | `TransferAmountPresenter.authorizeAndSubmit(validation:)` |
| Signing request sign, host and SSO | `PolkadotSigningPresenter.sign()` |
| Product-runtime signing confirmation | `ProductsSignConfirmPresenter.sign()` |
| Device pairing approve | `PolkadotSignInPresenter.approve()` |
| Product statement, VRF, proof and payment prompts | `StatementSignPromptViewFactory`, `SignVrfPromptViewFactory`, `CreateProofPromptViewFactory`, `PaymentRequestViewFactory` |
| Manual iCloud backup of the recovery phrase | `BackupPresenter.authorizeBackup()` |
| Reclaim of an unclaimed chat payment | `ChatPresenter.handleReclaimTransfer(for:)` |

The product prompts cover both the host API path and the product-runtime path, because both show the same prompts.

### Existing Device Authentication

These flows prompt today. They are not in the gated actions above. Account recovery, cloud restore and username claim pass `retriable: true`, so a cancel keeps the prompt open for another try (`LocalAuthPresenter`).

- Recovery phrase reveal: `BackupPresenter.authorize()`, in place.
- Account recovery: `AccountRecoveryPresenter.authorizeUser`.
- Cloud restore: `RestoreFromCloudPresenter.authorizeUser`.
- Username claim: `ClaimUsernamePresenter.authorizeUser`.

### Wiring

- A VIPER call site adds `AuthorizationPresentable` to its wireframe protocol. Examples: `PolkadotSignInWireframeProtocol`, `TransferAmountWireframeProtocol`.
- A prompt with no wireframe creates a `DeviceOwnerAuthorizer` (`polkadot-app/Modules/LocalAuth/DeviceOwnerAuthorizer.swift`). The approve closure captures it. `LocalAuthWireframe` then holds it strongly until the prompt completes.
- `LocalAuthViewFactory` builds the prompt view. `authorize` uses the full-screen view. `authorizeInPlace` uses a transparent view.

### Helper Behaviour

The helper lives in `LocalAuthPresentable`.

- A request while a prompt is live completes `false` at once.
- A request with no screen to present from completes `false` at once.
- Gates pass `retriable: false`. A cancel completes `false`. A failed prompt shows "authentication failed" first, then completes `false`.
- If the system cancels a live prompt, for example when the app leaves the foreground, the app maps it to a cancel. The screen stays in its pre-tap state.

### Device Authentication Wrapper

`DeviceAuthentication` keeps one `LAContext` for its lifetime. `LocalAuthViewFactory` creates a new instance for each prompt, so a context never outlives one prompt module. A retry reuses the context only after a failure or cancel. A success closes the module, so a context that already succeeded never passes again without a prompt.