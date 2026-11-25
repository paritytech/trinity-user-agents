// Loading surface for a product rendered as a dashboard widget while its
// webview page is still loading (phase 2). The whole-block skeleton pulse for
// the initial resolve (phase 1) lives on DashboardCardChrome; this fills the
// body region with the same opacity pulse so the loading treatment stays
// consistent. See docs/_plans/dashboard-widget-pulse-loading-design.md.
export const WidgetLoadingScreen = () => {
  return <div className="animate-widget-pulse h-full w-full bg-bg-surface-container" />;
};
