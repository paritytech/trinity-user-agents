package io.paritytech.polkadotapp.feature_products_impl.data.manifest

import com.google.gson.JsonDeserializationContext
import com.google.gson.JsonDeserializer
import com.google.gson.JsonElement
import io.paritytech.polkadotapp.feature_products_api.model.SemVer
import java.lang.reflect.Type

/**
 * RFC-0001 encodes a version as `[major, minor, patch]` or `[major, minor, patch, "build"]` — the
 * build element is a string, so the array is not homogeneous.
 */
class SemVerArrayAdapter : JsonDeserializer<SemVer> {
    override fun deserialize(json: JsonElement, typeOfT: Type, context: JsonDeserializationContext): SemVer {
        val parts = json.asJsonArray
        require(parts.size() in BASE_SIZE..WITH_BUILD_SIZE) { "appVersion must have 3 or 4 elements, got: $json" }

        return SemVer.fromComponents(
            major = parts[0].asInt,
            minor = parts[1].asInt,
            patch = parts[2].asInt,
            build = parts.takeIf { it.size() == WITH_BUILD_SIZE }?.get(BUILD_INDEX)?.asString,
        )
    }

    private companion object {
        const val BASE_SIZE = 3
        const val WITH_BUILD_SIZE = 4
        const val BUILD_INDEX = 3
    }
}
