package io.paritytech.polkadotapp.common.data.image.keyers

import coil.key.Keyer
import coil.request.Options
import io.paritytech.polkadotapp.common.data.image.loadables.JsImageLoadable

class JsImageKeyer : Keyer<JsImageLoadable> {
    override fun key(data: JsImageLoadable, options: Options): String = data.getCacheKey()
}
