import { describe, expect, test } from "bun:test";
import { walletPanel } from "./wallet-panel.js";

describe("wallet panel", () => {
  // With no wallet there is nothing to pick, sign in to, sign out of or forget.
  test("offers only import when no wallet is saved", () => {
    expect(walletPanel({ saved: 0, activeId: null, selectedId: "" })).toEqual({
      mode: "none",
      showPicker: false,
      showSignIn: false,
      showSignOut: false,
      showForget: false,
    });
  });

  // A saved wallet is not a signed-in one.
  test("offers a choice and Sign in when a wallet is saved but not in use", () => {
    expect(walletPanel({ saved: 1, activeId: null, selectedId: "a" })).toEqual({
      mode: "signed-out",
      showPicker: true,
      showSignIn: true,
      showSignOut: false,
      showForget: true,
    });
  });

  test("shows only Sign out for the one wallet that is signed in", () => {
    expect(walletPanel({ saved: 1, activeId: "a", selectedId: "a" })).toEqual({
      mode: "signed-in",
      showPicker: false,
      showSignIn: false,
      showSignOut: true,
      showForget: false,
    });
  });

  test("offers a switch and Forget for another wallet while signed in", () => {
    expect(walletPanel({ saved: 2, activeId: "a", selectedId: "b" })).toEqual({
      mode: "signed-in",
      showPicker: true,
      showSignIn: true,
      showSignOut: true,
      showForget: true,
    });
    expect(
      walletPanel({ saved: 2, activeId: "a", selectedId: "a" }),
    ).toMatchObject({ showPicker: true, showSignIn: false, showForget: false });
  });
});
