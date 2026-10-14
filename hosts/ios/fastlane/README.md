fastlane documentation
----

# Installation

Make sure you have the latest version of the Xcode command line tools installed:

```sh
xcode-select --install
```

For _fastlane_ installation instructions, see [Installing _fastlane_](https://docs.fastlane.tools/#installing-fastlane)

# Available Actions

## iOS

### ios base_build_app

```sh
[bundle exec] fastlane ios base_build_app
```

Build the iOS app

Parameters:

- 'scheme : <value>' defines scheme to use for build phase

- 'target : <value>' defines target to build

- 'configuration : <value>' defines configuration for build

- 'debug : true/false' to enable verbose logging for troubleshooting build failures

 

Example usage: fastlane build_app scheme:'polkadot-app' target: 'polkadot-app' configuration: 'Release' 

Example usage: fastlane base_build_app scheme:'polkadot-app' target: 'polkadot-app' configuration: 'DevCI' debug:true

### ios test_build

```sh
[bundle exec] fastlane ios test_build
```

Run tests

Parameters:

- 'scheme : <value>' to define scheme to test

- 'debug : true/false' to enable verbose logging for troubleshooting build failures (especially Crashlytics issues)

 

Example usage: fastlane test_build scheme:'polkadot-app'

Example usage: fastlane test_build scheme:'polkadot-app' debug:true

### ios distribute_to_firebase

```sh
[bundle exec] fastlane ios distribute_to_firebase
```

Distribute App to Firebase

Parameters:

- 'release_notes : <value>' to define release notes

Environment variables:

- FIREBASE_APP_ID - App ID of the app to distribute

- CREDENTIAL_FILE_CONTENT - Content of the credentials file for google service account

- FIREBASE_GROUPS - Groups to distribute the app to (comma separated)

 

Example usage: fastlane distribute_to_firebase release_notes:'Release notes' google_service_info_plist_path:'path/to/google-service-info.plist'

### ios register

```sh
[bundle exec] fastlane ios register
```

Register new devices

This lane will register new devices and update profiles via match

### ios upload_testflight

```sh
[bundle exec] fastlane ios upload_testflight
```

Submit a new build to Apple TestFlight

Example usage: fastlane upload_testflight changelog:'Release notes' distribute_external:true groups:['Group Name']

### ios load_asc_api_key

```sh
[bundle exec] fastlane ios load_asc_api_key
```

Load ASC API Key information to use in subsequent lanes

### ios get_testflight_build_number

```sh
[bundle exec] fastlane ios get_testflight_build_number
```

Get the latest build number from TestFlight

Example usage: fastlane get_testflight_build_number

### ios prepare_code_signing

```sh
[bundle exec] fastlane ios prepare_code_signing
```

Prepares certificate and provisioning profile

### ios update_signing_data

```sh
[bundle exec] fastlane ios update_signing_data
```

Updates signing data using App Store Connect API

Uses app_identifier from Matchfile, updates all profile types (development, adhoc, appstore)

### ios run_unit_tests

```sh
[bundle exec] fastlane ios run_unit_tests
```

Runs unit tests

Example usage: fastlane run_unit_tests

Example usage: fastlane run_unit_tests debug:true

### ios build_app_ci

```sh
[bundle exec] fastlane ios build_app_ci
```

Build app for CI

Parameters:

- 'debug : true/false' to enable verbose logging for troubleshooting build failures

 

Example usage: fastlane build_app_ci

Example usage: fastlane build_app_ci debug:true

### ios distribute_app_to_firebase

```sh
[bundle exec] fastlane ios distribute_app_to_firebase
```

Distribute app to Firebase

Parameters:

- 'release_notes : <value>' to define release notes

 

Example usage: fastlane distribute_app_to_firebase release_notes:'Release notes'

### ios distribute_testflight

```sh
[bundle exec] fastlane ios distribute_testflight
```

Distribute new iOS build through TestFlight

Requires BUILD_NUMBER env var to be set by the caller

Example usage: BUILD_NUMBER=42 fastlane distribute_testflight

### ios update_signing

```sh
[bundle exec] fastlane ios update_signing
```

Update signing data

Example usage: fastlane update_signing

----

This README.md is auto-generated and will be re-generated every time [_fastlane_](https://fastlane.tools) is run.

More information about _fastlane_ can be found on [fastlane.tools](https://fastlane.tools).

The documentation of _fastlane_ can be found on [docs.fastlane.tools](https://docs.fastlane.tools).
