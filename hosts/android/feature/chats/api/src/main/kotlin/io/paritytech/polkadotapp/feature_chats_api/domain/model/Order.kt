package io.paritytech.polkadotapp.feature_chats_api.domain.model

sealed interface Order : Comparable<Order> {
    val orderIndex: Int

    override fun compareTo(other: Order): Int {
        return orderIndex.compareTo(other.orderIndex)
    }

    object ByTimestamp : Order {
        override val orderIndex: Int = 1
    }
    object PinToTop : Order {
        override val orderIndex: Int = 0
    }
}
