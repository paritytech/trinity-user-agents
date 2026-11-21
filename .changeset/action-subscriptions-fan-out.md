---
"@parity/truapi-host": patch
---

Every open action subscription of a product connection now hears each action. Before, a new `actionSubscribe` on the same connection silently replaced the previous one, so a product that subscribed more than once only heard actions on its newest subscription. Closed subscriptions are dropped, and with none open, actions are still buffered for the next subscriber.
