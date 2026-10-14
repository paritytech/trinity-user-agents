# Copy to env-vars.sh (gitignored) and fill in real values for local builds.
#   cp polkadot-app/env-vars.template.sh polkadot-app/env-vars.sh
#
# These are consumed by Runscripts/generate_secrets.sh at build time. In CI the
# same variables are injected from GitHub Actions secrets, so env-vars.sh is not
# needed there. Never commit real values.

# Sentry crash/issue reporting (TESTNET_FEATURE builds only)
export SENTRY_DSN=""

# MELD fiat on-ramp Basic auth token ("publicKey:secretKey", base64-encoded)
export MELD_BASIC_AUTH_TOKEN=""
