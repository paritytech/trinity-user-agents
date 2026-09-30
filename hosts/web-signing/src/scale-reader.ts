/** A cursor over SCALE bytes that throws when a value is short, invalid or left over. */
export class Reader {
  private at = 0;
  constructor(private readonly bytes: Uint8Array) {}

  take(count: number): Uint8Array {
    if (this.at + count > this.bytes.length)
      throw new Error("the record ends early");
    const out = this.bytes.subarray(this.at, this.at + count);
    this.at += count;
    return out;
  }

  u8(): number {
    return this.take(1)[0];
  }

  u32(): number {
    return new DataView(this.take(4).slice().buffer).getUint32(0, true);
  }

  u64(): bigint {
    return new DataView(this.take(8).slice().buffer).getBigUint64(0, true);
  }

  compact(): number {
    const first = this.u8();
    switch (first & 0b11) {
      case 0:
        return first >> 2;
      case 1:
        return (first | (this.u8() << 8)) >> 2;
      case 2: {
        const rest = this.take(3);
        return (
          (first | (rest[0] << 8) | (rest[1] << 16) | (rest[2] << 24)) >>> 2
        );
      }
      default:
        throw new Error("a length is too large");
    }
  }

  text(): string {
    return new TextDecoder("utf-8", { fatal: true }).decode(
      this.take(this.compact()),
    );
  }

  finish(): void {
    if (this.at !== this.bytes.length)
      throw new Error("the record has unread bytes");
  }
}
