package io.paritytech.polkadotapp.feature_products_api.domain.pocket

import io.parity.truapi.MaterialsResolution
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget

/**
 * A card face and what its `Materials` nodes draw. [materials] is null for a face without one.
 * The source that emitted the face owns [materials] and closes it; a consumer only reads and reports.
 */
data class PocketFace(val widget: JsWidget, val materials: MaterialsResolution?)
