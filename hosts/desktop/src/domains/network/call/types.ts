/** A transaction call, decoded against the chain that will execute it. */
export type DecodedCall = {
  pallet: string;
  method: string;
  args: unknown;
};
