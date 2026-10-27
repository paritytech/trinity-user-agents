/** The developer tools' views, in the order their tabs show. */
export const DEV_VIEWS = [
  "account",
  "product",
  "preview",
  "inspector",
  "more",
] as const;
export type DevView = (typeof DEV_VIEWS)[number];

/** Where the developer tools sit on a wide screen. */
export type DevSide = "right" | "bottom";

/**
 * Whether the developer tools are open, and which view they show.
 *
 * On a wide screen they sit beside the app, and closing them is Focus: the
 * app gets the whole window. On a narrow screen they cover the app, so they
 * take focus away from it until they are closed.
 */
export class Layout {
  private shown: boolean;
  private current: DevView;
  private modalPanel = false;

  constructor(open: boolean, view: DevView = "account") {
    this.shown = open;
    this.current = view;
  }

  get open(): boolean {
    return this.shown;
  }

  get view(): DevView {
    return this.current;
  }

  /** Whether `view` shows now. */
  isShown(view: DevView): boolean {
    return this.shown && this.current === view;
  }

  /** Whether the open tools cover the app. */
  get covering(): boolean {
    return this.modalPanel && this.shown;
  }

  /** Switch between beside-the-app and over-the-app tools. */
  setModal(modal: boolean): void {
    this.modalPanel = modal;
  }

  show(view: DevView): void {
    this.shown = true;
    this.current = view;
  }

  /** Show `view`, or close the tools when it already shows. */
  toggle(view: DevView): void {
    if (this.isShown(view)) this.shown = false;
    else this.show(view);
  }

  /** Open the tools at the view they last showed, or close them. */
  toggleOpen(): void {
    this.shown = !this.shown;
  }

  close(): void {
    this.shown = false;
  }

  /** Close the tools when they cover the app; false when nothing does. */
  dismissCovering(): boolean {
    if (!this.covering) return false;
    this.shown = false;
    return true;
  }
}
