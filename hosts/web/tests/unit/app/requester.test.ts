import { describe, expect, test } from "bun:test";
import { describeRequester } from "../../../src/app/requester.js";

describe("describeRequester", () => {
  test("says nothing when no product the host runs has the id", () => {
    expect(describeRequester("localhost", [])).toBeNull();
  });

  // The id `localhost` alone does not say which page asks; the address the
  // host opened it from does.
  test("names the address a product was opened from when the id does not say", () => {
    expect(
      describeRequester("localhost", [
        { kind: "App", address: "http://localhost:5173/play" },
      ]),
    ).toBe("App opened from http://localhost:5173/play");
    expect(
      describeRequester("myapp.paseo", [
        { kind: "App", address: "http://localhost:5173/" },
      ]),
    ).toBe("App opened from http://localhost:5173/");
  });

  test("adds nothing that would only repeat the id", () => {
    expect(
      describeRequester("localhost:5173", [
        { kind: "App", address: "http://localhost:5173/" },
      ]),
    ).toBeNull();
    expect(
      describeRequester("doom.paseo", [{ kind: "App", address: "doom.paseo" }]),
    ).toBeNull();
  });

  test("keeps credentials and secret parameters out of the address", () => {
    expect(
      describeRequester("myapp.paseo", [
        { kind: "App", address: "http://user:pw@localhost:1/?token=abc&x=1" },
      ]),
    ).toBe("App opened from http://localhost:1/?token=%E2%80%A6&x=1");
  });

  test("says a worker is run by the host", () => {
    expect(
      describeRequester("worker.paseo", [{ kind: "Worker", address: null }]),
    ).toBe("Worker run by this host");
  });

  // Two products under one id are one product to the core, so a label cannot
  // tell them apart and the host does not pretend it can.
  test("says it cannot tell products that share an id apart", () => {
    expect(
      describeRequester("localhost", [
        { kind: "App", address: "http://localhost:5173/" },
        { kind: "Worker", address: null },
      ]),
    ).toBe(
      "2 open products share this id, so the host cannot tell which one asks.",
    );
  });
});
