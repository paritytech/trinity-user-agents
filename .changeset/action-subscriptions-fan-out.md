---
"@parity/truapi-host": patch
---

Every open action subscription of a product connection now hears each action. Before, a new `actionSubscribe` on the same connection silently replaced the previous one, so a product that subscribed more than once only heard actions on its newest subscription. Closed subscriptions are dropped, and with none open, actions are still buffered for the next subscriber. Actions buffered while no subscription was open still go only to the next subscription, not to every one opened after it; a follow-up will handle those. A product that calls `actionSubscribe` again without ending its old stream now gets each action on both streams, where the old stream used to end.
