import type {
  HostNotification,
  NotificationStore,
} from "../../src/notifications/notifications.js";

/**
 * DEV ONLY. Made-up notifications for the Notifications screen. The people
 * and products are invented and reach nothing; their product ids end in
 * `.example` so they read as no published name.
 *
 * Times are relative to `now`: four requests wait, one of them expiring two
 * minutes from now, and three are history, one accepted, one declined and
 * one expired.
 */
export function sampleNotifications(now: number): HostNotification[] {
  const minute = 60_000;
  const hour = 60 * minute;
  const day = 24 * hour;
  const chat = { id: "chat.example", name: "Example Chat" };
  const profiles = { id: "profiles.example", name: "Example Profiles" };
  const base = {
    seenAt: null,
    state: { tag: "pending" as const },
  };
  return [
    {
      ...base,
      id: "demo-invite-ada",
      kind: "invitation",
      sender: { name: "Ada", handle: "ada.example" },
      product: chat,
      purpose: "Ada wants to start a chat with you.",
      receivedAt: now - 4 * minute,
      expiresAt: now + 2 * day,
    },
    {
      ...base,
      id: "demo-share-grace",
      kind: "sharing",
      sender: { name: "Grace", handle: "grace.example" },
      product: profiles,
      purpose:
        "Grace asks to see your profile name and picture, to show them next to your messages in a shared project.",
      receivedAt: now - 40 * minute,
      expiresAt: now + day,
    },
    {
      ...base,
      id: "demo-invite-soon",
      kind: "invitation",
      sender: { name: "Lin", handle: "lin.example" },
      product: chat,
      purpose: "Lin invites you to a chat. This invitation expires soon.",
      receivedAt: now - 2 * hour,
      expiresAt: now + 2 * minute,
    },
    {
      ...base,
      id: "demo-share-long",
      kind: "sharing",
      sender: {
        name: "Maximiliana Konstantinopoulou-Abernathy of the Northern Reading Circle",
        handle: "maximiliana-konstantinopoulou-abernathy.example",
      },
      product: profiles,
      purpose:
        "We would like to see your profile name, your picture and the short description you wrote, so that the reading circle can list you with the other members, show who suggested which book this month, and send you the schedule for the next three meetings. Nothing else is asked for.",
      receivedAt: now - 6 * hour,
      expiresAt: now + 3 * day,
    },
    {
      ...base,
      id: "demo-invite-accepted",
      kind: "invitation",
      sender: { name: "Sam", handle: "sam.example" },
      product: chat,
      purpose: "Sam wants to start a chat with you.",
      receivedAt: now - 2 * day,
      expiresAt: now + day,
      seenAt: now - 2 * day + hour,
      state: { tag: "accepted", at: now - 2 * day + hour },
    },
    {
      ...base,
      id: "demo-share-declined",
      kind: "sharing",
      sender: { name: "Kim", handle: "kim.example" },
      product: profiles,
      purpose: "Kim asks to see your profile name.",
      receivedAt: now - 3 * day,
      expiresAt: now + day,
      seenAt: now - 3 * day + hour,
      state: { tag: "declined", at: now - 3 * day + hour },
    },
    {
      ...base,
      id: "demo-invite-expired",
      kind: "invitation",
      sender: { name: "Noor", handle: "noor.example" },
      product: chat,
      purpose: "Noor wants to start a chat with you.",
      receivedAt: now - 5 * day,
      expiresAt: now - 4 * day,
      seenAt: now - 5 * day + hour,
    },
  ];
}

const SAMPLE_IDS = sampleNotifications(0).map((item) => item.id);

/** Put the samples in `store`, fresh, in place of any added before. */
export function addSamples(store: NotificationStore): void {
  store.remove(SAMPLE_IDS);
  for (const item of sampleNotifications(store.now())) store.add(item);
}

/** Take the samples out of `store`, and nothing else. */
export function clearSamples(store: NotificationStore): void {
  store.remove(SAMPLE_IDS);
}
