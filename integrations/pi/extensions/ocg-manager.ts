import { readFileSync } from "node:fs";
import { createProvider, openAICompletionsApi, type Model } from "@earendil-works/pi-ai";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

const providerId = "ocg-manager";
const baseUrl = "http://127.0.0.1:9042/v1";

type GeneratedCatalog = {
  version: 1;
  models: Model<"openai-completions">[] | string;
};

function loadGeneratedModels(): Model<"openai-completions">[] {
  const catalog = JSON.parse(
    readFileSync(new URL("../models.generated.json", import.meta.url), "utf8"),
  ) as GeneratedCatalog;

  if (!Array.isArray(catalog.models)) {
    throw new Error("Open Console Gateway model catalog has not been generated.");
  }

  return catalog.models;
}

export default function registerOcgManagerProvider(pi: ExtensionAPI) {
  pi.registerProvider(
    createProvider({
      id: providerId,
      name: "Open Console Gateway",
      baseUrl,
      auth: {
        apiKey: {
          name: "Open Console Gateway API key",
          async login(interaction) {
            return {
              type: "api_key",
              key: await interaction.prompt({
                type: "secret",
                message: "Open Console Gateway API key",
              }),
            };
          },
          async resolve({ credential }) {
            return credential?.key
              ? { auth: { apiKey: credential.key }, source: "stored API key" }
              : undefined;
          },
        },
      },
      models: loadGeneratedModels(),
      api: openAICompletionsApi(),
    }),
  );
}
