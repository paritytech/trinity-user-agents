/** A panel that can be shown next to the product. */
export type Panel = "menu" | "inspector";

/**
 * Which panels are open.
 *
 * On a wide screen the panels sit beside the product and open independently.
 * On a narrow screen they cover it, so one is open at a time and it takes
 * focus away from the product until it is closed.
 */
export class Layout {
  private open: Set<Panel>;
  private modalPanels = false;

  constructor(initial: Iterable<Panel> = []) {
    this.open = new Set(initial);
  }

  isOpen(panel: Panel): boolean {
    return this.open.has(panel);
  }

  /** The panel covering the product, if the screen is narrow and one is open. */
  get covering(): Panel | null {
    if (!this.modalPanels) return null;
    return [...this.open][0] ?? null;
  }

  /** Switch between beside-the-product and over-the-product panels. */
  setModal(modal: boolean): void {
    this.modalPanels = modal;
    if (modal && this.open.size > 1) this.open = new Set([[...this.open][0]]);
  }

  toggle(panel: Panel): void {
    if (this.open.has(panel)) this.close(panel);
    else this.show(panel);
  }

  show(panel: Panel): void {
    if (this.modalPanels) this.open.clear();
    this.open.add(panel);
  }

  close(panel: Panel): void {
    this.open.delete(panel);
  }

  /** Close the covering panel; false when nothing covers the product. */
  dismissCovering(): boolean {
    const panel = this.covering;
    if (panel === null) return false;
    this.close(panel);
    return true;
  }
}
