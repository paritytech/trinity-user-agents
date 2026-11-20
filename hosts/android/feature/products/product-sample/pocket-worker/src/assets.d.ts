// esbuild's binary loader hands a bundled image over as its bytes.
declare module "*.png" {
  const bytes: Uint8Array;
  export default bytes;
}
