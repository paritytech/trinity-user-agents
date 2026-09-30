/**
 * Where the wallet section stands. Saved wallets and the signed-in wallet are
 * separate facts: a wallet can be saved without being signed in.
 */
export type WalletMode = "none" | "signed-out" | "signed-in";

export interface WalletPanelInput {
  /** How many usable wallets are saved. */
  saved: number;
  /** The wallet this tab is signed in with, if any. */
  activeId: string | null;
  /** The wallet the picker points at, or "" when it points at none. */
  selectedId: string;
}

/** Which wallet controls apply right now. */
export interface WalletPanel {
  mode: WalletMode;
  showPicker: boolean;
  showSignIn: boolean;
  showSignOut: boolean;
  showForget: boolean;
}

/**
 * Decide which wallet controls to show, so a control that cannot do anything
 * is not shown at all. Signing out stays available while signed in. Forgetting
 * is offered only for a wallet other than the one in use.
 */
export function walletPanel(input: WalletPanelInput): WalletPanel {
  const { saved, activeId, selectedId } = input;
  if (saved === 0)
    return {
      mode: "none",
      showPicker: false,
      showSignIn: false,
      showSignOut: false,
      showForget: false,
    };
  const other = selectedId !== "" && selectedId !== activeId;
  if (activeId === null)
    return {
      mode: "signed-out",
      showPicker: true,
      showSignIn: true,
      showSignOut: false,
      showForget: selectedId !== "",
    };
  return {
    mode: "signed-in",
    showPicker: saved > 1,
    showSignIn: other,
    showSignOut: true,
    showForget: other,
  };
}
