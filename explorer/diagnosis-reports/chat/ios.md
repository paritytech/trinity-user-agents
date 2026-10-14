## Truapi iOS Chat Diagnosis

**6 success · 1 failed**

| Method | Status | Details |
| --- | --- | --- |
| `Chat/create_room` | ✅ | created once, then returned Exists |
| `Chat/register_bot` | ❌ | registerBot failed: bot registration is not supported by this host |
| `Chat/list_subscribe` | ✅ | observed the newly created room |
| `Chat/post_message` | ✅ | posted text and custom messages |
| `Chat/action_subscribe` | ✅ | received MessagePosted with the originating room |
| `Renderer/render` | ✅ | served initial and replacement trees on a host-initiated render stream |
| `Renderer/action_subscribe` | ✅ | renderer action stream is open; a press inside the rendered tree is delivered on it |
