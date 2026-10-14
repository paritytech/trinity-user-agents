package io.paritytech.polkadotapp.feature_dotns_gateway_api.domain.model

sealed interface DotNsBaseNameAvailability {
    data object Free : DotNsBaseNameAvailability

    data object ReservedByUs : DotNsBaseNameAvailability

    data object TakenByOther : DotNsBaseNameAvailability
}
