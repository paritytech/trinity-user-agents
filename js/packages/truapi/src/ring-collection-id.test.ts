import { expect, test } from "bun:test";
import { ringCollectionId } from "./ring-collection-id.js";

test("collection ids match the identifiers the People chain stores", () => {
    expect({
        people: ringCollectionId("pop:polkadot.network/people"),
        peopleLite: ringCollectionId("pop:polkadot.network/people-lite"),
    }).toEqual({
        people: "0x706f703a706f6c6b61646f742e6e6574776f726b2f70656f706c652020202020",
        peopleLite: "0x706f703a706f6c6b61646f742e6e6574776f726b2f70656f706c652d6c697465",
    });
});

test("a name longer than 32 bytes is rejected instead of naming another ring", () => {
    expect(() => ringCollectionId("pop:polkadot.network/people-lite!")).toThrow(RangeError);
});
