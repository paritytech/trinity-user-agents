> [!WARNING]
> The following is a proof-of-concept. This open source code is provided for research, experimentation, and developer education only. This code has not been audited, is actively experimental, and may contain bugs, vulnerabilities, or incomplete features. Use at your own risk and obtain legal advice as appropriate - DYOR.

Parity doesn’t deploy the code but may update it based on community feedback.

If you experience problems with any product or service that was built on or deployed from this code, you should contact the third party who deployed the code in its amended form, not Parity.

<div align="center">

# Polkadot iOS

*Self-custodial iOS superapp for Polkadot. Messaging, identity, payments, and built-in support for Polkadot applications — all-in-one, with you in full control of it.*

[![License](https://img.shields.io/badge/license-GPL--3.0-blue?style=flat-square)](./LICENSE)
[![Platform](https://img.shields.io/badge/iOS-17.0%2B-black?style=flat-square&logo=apple)](https://developer.apple.com/ios/)
[![Swift](https://img.shields.io/badge/swift-5.10-F05138?style=flat-square&logo=swift)](https://swift.org)
[![Polkadot](https://img.shields.io/badge/polkadot-ecosystem-E6007A?style=flat-square&logo=polkadot)](https://polkadot.com)

<img src="docs/screenshots/hero-banner.png" alt="Polkadot iOS — self-custodial superapp showing the chats list and identity card on an iPhone" width="800">

</div>

## Features

- **Identity** — On-chain username with an allowance for free transactions, verified by Proof-of-Unique-Device.
- **Personhood** — Upgrade your username to a higher allowance via Proof-of-Personhood by playing the DIM2 videocall gesture game. Game rewards and collectibles can be surfaced through dApps (behind a feature flag).
- **Chat** — End-to-end p2p encrypted text messaging with media (images/video) and encrypted video/audio calls.
- **Payments** — Send and receive payments by username or QR code, and directly in chat.
- **Auto-conversion** — Top up your wallet and auto-convert it into the tokens you want.
- **Built-in dApp support & sandboxing** — Explore and use any dApp, and manage its sandbox permissions.
- **dApp modalities** — Supports SPA and Chat modalities for dApps.
- **Deeplinks** — Navigate the app and dApps through links and QR codes.
- **Remote signing** — Connect to Polkadot Desktop and Polkadot Web and use Mobile as a signer.
- **Multi-device sync** — Sync contacts and chats between Polkadot Mobile and Polkadot Desktop.
- **Cloud backups** — Back up your account using your iCloud keystore or Google Drive.
- **Manual backups** — Keep your account stored only in the secure enclave storage locally.
- **Customization** — Fully customizable UI design system with 5 default themes.

## Getting started

<details>
<summary>Prerequisites</summary>

- **Xcode** with the iOS 17.0+ SDK
- `make ios-bootstrap` from the repository root, once per clone, before opening
  the project. The shared core resolves from this tree, and its bindings are
  gitignored build outputs, so the package graph cannot load until they exist.
- Other Swift Package Manager dependencies resolve automatically on first build

</details>

Clone the repo, scaffold the local config, and open the project in Xcode:

```bash
git clone https://github.com/paritytech/polkadot-ios-community.git
cd polkadot-ios-community

# Scaffold gitignored secret files from templates and generate build-time config.
./Runscripts/setup-secrets.sh

open polkadot-app.xcodeproj
```

The project compiles out of the box with empty secrets and placeholder Firebase
plists. Select the **polkadot-app** scheme and an iOS 17+ simulator, then build
and run (`Cmd+R`).

To get past the startup screen the app needs a **Firebase project of your own**:
the chain set, the identity-backend URL, the DotNS contract addresses and the
other runtime endpoints are all delivered through Firebase Remote Config, and
the placeholder plists point at nothing. [docs/PUBLISHING.md](./docs/PUBLISHING.md)
lists every Remote Config parameter and build-time variable and the brand
identity keys in `Configs/brand.xcconfig`; the identity backend is open source
as [device-uniqueness-backend-community](https://github.com/paritytech/device-uniqueness-backend-community). Optional
integrations (fiat on-ramp, crash reporting) are enabled by filling in
`polkadot-app/env-vars.sh`.

The app talks to Polkadot system chains (People Chain, Asset Hub, Bulletin Chain); the chain set is
delivered via remote config, and development and nightly builds are exercised against Polkadot's
[Paseo](https://github.com/paseo-network) testnet contour.

### Build and test from the command line

```bash
# Build the app
xcodebuild -project polkadot-app.xcodeproj -scheme polkadot-app -configuration Debug build

# Run unit tests
xcodebuild test -project polkadot-app.xcodeproj -scheme polkadot-appTests \
  -destination 'platform=iOS Simulator,name=iPhone 16'
```

## How it works

Polkadot iOS is a self-custodial superapp: your keys are created on your phone, stay on your phone, and everything else — identity, chat, payments, apps — is built on top of them using Polkadot's public chains instead of company servers.

### What it does

1. **Keeps your keys on your device.** Your account is generated locally and protected by the device's secure enclave and Keychain. You choose how to back it up: an encrypted backup in your own iCloud or Google Drive, or no backup at all — keys stored only on the device.
2. **Gives you an on-chain name.** You register a username on Polkadot's [People Chain](https://wiki.polkadot.com/learn/learn-system-chains/). Your phone proves it's a unique device, which earns you an allowance for free transactions — no tokens needed to start. Prove personhood by playing the DIM2 videocall gesture game to raise that allowance.
3. **Lets you chat without a messaging server.** Messages are end-to-end encrypted and delivered through the People Chain statement store, so there is no company inbox holding your conversations. Voice and video calls are encrypted and go directly peer-to-peer over WebRTC.
4. **Sends money to names, not addresses.** Pick a username (or scan a QR code, or pay right inside a chat) — the app resolves it to an account on-chain and sends the payment. Swaps and auto-conversion run on [Asset Hub](https://wiki.polkadot.com/learn/learn-assets/)'s liquidity pools.
5. **Runs Polkadot apps inside the app.** Type a `.dot` name and the app fetches the dApp's content (published on the Bulletin Chain and addressed via DotNS) and runs it in a sandbox. Each dApp gets its own permissions — network, camera, signing, storage — that you grant and revoke per app.
6. **Works as one account across devices.** Pair with Polkadot Desktop or Polkadot Web by scanning a QR code: your phone becomes the signer that approves their transactions, and contacts and chats sync between devices over the same encrypted channels.

### What it doesn't do

- There is no custodian, and nobody (including the developers) can freeze, recover, or move your account. If you lose your device and have no backup, the account is gone.
- It does **not** route your chats and calls through any 1st or 3rd party messaging servers — messages travel through the public chain, voice and video calls go peer-to-peer.
- It is **not** a production-hardened product — treat it as a reference implementation (see the warning at the top).

### Under the hood

Built with **UIKit** and programmatic layout (no Storyboards), using **VIPER** for every feature module: code is split between the main app target and 28 local Swift packages under [`Packages/`](./Packages) with `AppDependencies` as the root package, chain access goes through [substrate-sdk-ios](https://github.com/novasamatech/substrate-sdk-ios) (JSON-RPC, storage subscriptions, extrinsics), and local data lives in CoreData.

This repository ships a **GitHub Actions + Fastlane CI/CD setup** — PR build and
tests, plus maintainer-gated TestFlight and Firebase App Distribution. Build-time
configuration, signing, the required secrets, and the pipeline itself are
documented in [docs/PUBLISHING.md](./docs/PUBLISHING.md). A fork must supply its
own secrets, signing repo, and runners before the pipeline runs green.

Architecture conventions, module layout, and coding standards are documented in [CLAUDE.md](./CLAUDE.md).
Working on a Pocket card, through the face-preview loop and the live-card loop, is documented in
[docs/pocket-card-dev-loop.md](./docs/pocket-card-dev-loop.md).

## Contributing

Issues and pull requests are welcome. Read [CONTRIBUTING.md](./CONTRIBUTING.md) before you start.

## Security

This is a prototype, reference implementation, and proof-of-concept provided for research,
experimentation, and developer education only. It has not been audited — use at your own risk.

Before deploying this for real use cases, you are responsible for:

- Reviewing the code yourself — we publish a reference, not a hardened production build.
- Checking that the dependencies are up to date and free of known vulnerabilities.
- Securing your own fork or deployment environment (keys, secrets, network configuration).
- Tracking the latest commits for security fixes; older revisions are not backported.

Report vulnerabilities responsibly following [Parity's security policy](https://github.com/paritytech/.github/blob/main/SECURITY.md) — do not open public issues for security reports. For Parity's disclosure process and Bug Bounty programme, see [parity.io/bug-bounty](https://parity.io/bug-bounty).

## License

Licensed under the **GNU General Public License v3.0** — see [LICENSE](./LICENSE).
