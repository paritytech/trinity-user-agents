package io.paritytech.polkadotapp.feature_usernames_api.domain.model

data class Username private constructor(
    val base: String,
    val index: Int?,
) {
    val isFull: Boolean get() = index == null

    fun getDisplayUsername(): String {
        return buildString {
            append(base)
            if (index != null) {
                append(SEPARATOR)
                append(INDEX_FORMAT.format(index))
            }
        }
    }

    companion object {
        const val SEPARATOR = '.'
        private const val INDEX_FORMAT = "%02d"

        fun fromParts(username: String, index: Int?): Username {
            return Username(base = username, index = index)
        }

        fun fromFullValue(fullUsername: String): Username {
            val username = fullUsername.substringBeforeLast(SEPARATOR)

            val index = fullUsername
                .substringAfterLast(SEPARATOR, missingDelimiterValue = "")
                .toIntOrNull()

            return Username(username, index)
        }
    }
}

fun Username.encoded(): ByteArray {
    return getDisplayUsername().encodeToByteArray()
}
