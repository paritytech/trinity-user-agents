# End to end scenarios on the headless CLI host

Each `<name>.sh` here is one scenario: it builds `truapi-host` from the checkout, runs it with the product script and fixtures it needs from `rust/crates/truapi-host-cli/js/`, and exits non-zero when the scenario fails. Every script runs from anywhere and explains at its top what it proves and which environment variables it reads.

Run one directly, or through make:

```bash
e2e/cli/cross-product-storage.sh
make e2e-cli SCENARIO=cross-product-storage
make e2e-cli SCENARIO=battery ARGS=--signing-host   # ARGS go to the script
make e2e-cli                                       # list the scenarios
```

- `battery.sh`: the generated full-surface battery, with a phase per host role (`--signing-host`, `--pairing-host`, `--chat-host`, `--pocket-host`, `--funding-host`). The signing and pairing phases write the committed reports under `explorer/diagnosis-reports/spa/`. `--help` lists its options.
- `cross-product-*.sh`: one product uses another's storage, keys or accounts under a manifest grant.
- `preimage-submit-prompt.sh`: preimage uploads under a `PreimageSubmit` grant ask for no per-upload confirmation.

## Adding a scenario

Add an executable `<name>.sh` here, and put the TypeScript it drives in `rust/crates/truapi-host-cli/js/`. `make e2e-cli` picks it up by its file name, so it needs no Makefile target. Only an end to end check that depends on another make target, such as `e2e-cli-update` on `cli-dist`, gets a target of its own.
