import { describe, expect, test } from "bun:test";
import { services as generatedServices } from "@parity/truapi/playground/services";
import { servicesForExecution } from "@parity/truapi/playground/services-types";
import { WORKER_DIAGNOSIS_METHODS, ChatDiagnosis } from "../../worker/diagnosis";

/**
 * Worker-pinned methods the playground deliberately leaves out of its
 * diagnosis. The CLI battery drives Pocket over the real wire instead. Naming
 * each method rather than the whole service keeps the guard on the rest of it,
 * so a third Pocket method fails the check below until it is either diagnosed
 * or listed here.
 */
const UNDIAGNOSED_WORKER_METHODS: ReadonlySet<string> = new Set([
  "Pocket/list_subscribe",
  "Pocket/remove_card",
]);

describe("ChatDiagnosis", () => {
  // Expectation comes from codegen, so a missing method fails here. The
  // Worker service is the worker's own operation lifecycle rather than a
  // diagnosis step, so the report does not cover it.
  test("covers every generated Worker method", () => {
    const generated = servicesForExecution(generatedServices, "Worker")
      .filter(
        (service) =>
          service.requiredExecution === "Worker" && service.name !== "Worker",
      )
      .flatMap((service) =>
        service.methods.map((method) => `${service.name}/${method.name}`),
      )
      .filter((id) => !UNDIAGNOSED_WORKER_METHODS.has(id));

    expect(generated.length).toBeGreaterThan(0);
    expect([...WORKER_DIAGNOSIS_METHODS].sort()).toEqual(generated.sort());
  });

  test("renders a Chat-only report over every tracked method", () => {
    const diagnosis = new ChatDiagnosis();
    for (const id of WORKER_DIAGNOSIS_METHODS) {
      diagnosis.pass(id, "worked");
    }

    expect(diagnosis.isComplete()).toBe(true);
    expect(diagnosis.markdown()).toContain("## Truapi Chat Diagnosis");
    expect(diagnosis.markdown()).toContain(
      `**${WORKER_DIAGNOSIS_METHODS.length} success · 0 failed**`,
    );
    expect(diagnosis.markdown()).not.toContain("Storage/");
  });

  test("reports failures without preventing renderer output", () => {
    const diagnosis = new ChatDiagnosis();
    diagnosis.fail("Chat/create_room", new Error("room unavailable"));
    diagnosis.pass("Chat/create_room", "late success");

    expect(diagnosis.markdown()).toContain("❌ | room unavailable");
    expect(diagnosis.markdown()).not.toContain("late success");
    expect(diagnosis.rendererNode().tag).toBe("Column");
  });

  test("identifies the custom-rendered panel separately from the text report", () => {
    const diagnosis = new ChatDiagnosis();
    const renderer = JSON.stringify(diagnosis.rendererNode());

    expect(renderer).toContain("NATIVE CUSTOM MESSAGE");
    expect(renderer).toContain("Custom message rendered ✓");
    expect(renderer).toContain(
      "This panel is a live native renderer tree from TrUAPI Playground.",
    );
    expect(diagnosis.markdown()).not.toContain("Custom message rendered");
  });

  test("renders a copy action and reports clipboard fallback state", () => {
    const diagnosis = new ChatDiagnosis();
    const initial = JSON.stringify(diagnosis.rendererNode());

    expect(initial).toContain("Copy report");
    expect(initial).toContain("truapi-chat-diagnosis-copy");

    diagnosis.copyUnavailable();
    expect(JSON.stringify(diagnosis.rendererNode())).toContain(
      "long-press the report message below",
    );

    diagnosis.copied();
    expect(JSON.stringify(diagnosis.rendererNode())).toContain("Copied ✓");
  });
});
