// Side-effect CSS imports (e.g. `import "./globals.css"`) carry no types;
// declare the module so TypeScript accepts them.
declare module "*.css";

// The editor core without the language services. It ships no typings of its
// own, and exports the same API as the package entry.
declare module "monaco-editor/esm/vs/editor/edcore.main" {
  export * from "monaco-editor";
}
