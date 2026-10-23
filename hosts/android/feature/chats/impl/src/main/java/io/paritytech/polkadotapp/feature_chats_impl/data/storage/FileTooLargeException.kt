package io.paritytech.polkadotapp.feature_chats_impl.data.storage

import io.paritytech.polkadotapp.common.utils.InformationSize

class FileTooLargeException(
    val actualSize: InformationSize,
    val maxSize: InformationSize
) : IllegalArgumentException("File size $actualSize exceeds maximum allowed $maxSize")
