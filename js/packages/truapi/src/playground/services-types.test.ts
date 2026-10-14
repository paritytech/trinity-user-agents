import { describe, expect, test } from "bun:test";
import { servicesForExecution, type ServiceInfo } from "./services-types.js";
import { services as generatedServices } from "./codegen/services.js";

const services: ServiceInfo[] = [
    { name: "Storage", methods: [] },
    { name: "Chat", requiredExecution: "Worker", methods: [] },
];

describe("servicesForExecution", () => {
    test("keeps shared services and services for the selected execution", () => {
        expect(servicesForExecution(services, "App").map(({ name }) => name)).toEqual(["Storage"]);
        expect(servicesForExecution(services, "Widget").map(({ name }) => name)).toEqual([
            "Storage",
        ]);
        expect(servicesForExecution(services, "Worker").map(({ name }) => name)).toEqual([
            "Storage",
            "Chat",
        ]);
    });

    test("generated Chat metadata carries its trusted execution requirement", () => {
        expect(generatedServices.find(({ name }) => name === "Chat")?.requiredExecution).toBe(
            "Worker",
        );
        expect(
            generatedServices.find(({ name }) => name === "Storage")?.requiredExecution,
        ).toBeUndefined();
    });

    test("generated metadata identifies host-initiated subscriptions", () => {
        const renderer = generatedServices.find(({ name }) => name === "Renderer");
        expect(renderer?.requiredExecution).toBe("Worker");
        expect(renderer?.methods.find(({ name }) => name === "render")?.hostInitiated).toBe(true);
        expect(
            renderer?.methods.find(({ name }) => name === "action_subscribe")?.hostInitiated,
        ).toBeUndefined();
    });
});
