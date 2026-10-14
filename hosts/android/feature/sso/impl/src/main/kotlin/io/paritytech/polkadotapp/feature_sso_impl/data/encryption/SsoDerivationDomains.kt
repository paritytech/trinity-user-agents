package io.paritytech.polkadotapp.feature_sso_impl.data.encryption

import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain

object SsoDerivationDomains {
    val SSO_DERIVATION_DOMAIN = SharedSecretDerivationDomain("sso")
}
