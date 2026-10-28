/**
 * The card the Docklet's Pill opens: the host's search field, where a
 * product is opened by name or URL and a person is found by name or username,
 * with its suggestions. It draws nothing of its own; it only shows, hides and
 * gives focus back.
 */
export class SearchCard {
  constructor(
    private readonly pill: HTMLButtonElement,
    private readonly root: HTMLElement,
    private readonly input: HTMLInputElement,
    close: HTMLButtonElement,
    /** Called each time the card closes. */
    private readonly onClose: () => void = () => {},
  ) {
    pill.addEventListener("click", () =>
      this.isOpen() ? this.close(true) : this.open(),
    );
    close.addEventListener("click", () => this.close(true));
    root.addEventListener("keydown", (event) => {
      if (event.key !== "Escape" || event.target === input) return;
      event.preventDefault();
      event.stopPropagation();
      this.close(true);
    });
    document.addEventListener(
      "pointerdown",
      (event) => {
        const target = event.target as Node | null;
        if (
          this.isOpen() &&
          target !== null &&
          !root.contains(target) &&
          !pill.contains(target)
        )
          this.close(false);
      },
      true,
    );
  }

  isOpen(): boolean {
    return !this.root.hidden;
  }

  open(): void {
    this.root.hidden = false;
    document.body.dataset.search = "open";
    this.pill.setAttribute("aria-expanded", "true");
    this.input.focus();
    this.input.select();
  }

  /** Hide the card, and give focus back to the Pill when it was inside or `returnFocus`. */
  close(returnFocus: boolean): void {
    if (!this.isOpen()) return;
    const inside = this.root.contains(document.activeElement);
    this.root.hidden = true;
    document.body.dataset.search = "closed";
    this.pill.setAttribute("aria-expanded", "false");
    this.onClose();
    if (returnFocus || inside) this.pill.focus();
  }
}
