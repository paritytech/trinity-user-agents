/// <reference path="../runner.ts" />
export {};

const login = await truapi.account.requestLogin({ reason: undefined });
if (
  !login.isOk() ||
  (login.value !== "Success" && login.value !== "AlreadyConnected")
) {
  throw new Error(
    `requestLogin failed: ${login.isOk() ? login.value : JSON.stringify(login.error)}`,
  );
}

const statuses: string[] = [];
await new Promise<void>((resolve, reject) => {
  let subscription: { unsubscribe(): void } | undefined;
  subscription = truapi.account.connectionStatusSubscribe().subscribe({
    next(status) {
      statuses.push(status);
      if (status === "Connected") {
        console.log("DEVICE_REMOVE_CONNECTED");
      }
      if (status === "Disconnected") {
        subscription?.unsubscribe();
        resolve();
      }
    },
    error(error) {
      reject(error);
    },
  });
});

const expectedStatuses = ["Connected", "Disconnected"];
assert(
  JSON.stringify(statuses) === JSON.stringify(expectedStatuses),
  "unexpected account connection statuses",
  statuses,
);
console.log("DEVICE_REMOVE_DISCONNECT_OK");

await new Promise<never>(() => {});
