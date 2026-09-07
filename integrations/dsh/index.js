import { findPackageJSON } from "node:module";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";

if (process.argv[1] === undefined) {
  throw new Error("Open Console Gateway could not identify the active DSH runtime.");
}
const runtimeBase = pathToFileURL(process.argv[1]).href;
async function importFromDsh(packageName, entry) {
  const manifest = findPackageJSON(packageName, runtimeBase);
  if (manifest === undefined) {
    throw new Error(`Open Console Gateway requires ${packageName} from the active DSH runtime.`);
  }
  return import(pathToFileURL(join(dirname(manifest), entry)).href);
}

const [piAi, openAiApi, dshLlm, dshPiAi] = await Promise.all([
  importFromDsh("@earendil-works/pi-ai", "dist/index.js"),
  importFromDsh(
    "@earendil-works/pi-ai",
    "dist/api/openai-completions.lazy.js",
  ),
  importFromDsh("@deepseek-ai/dsh-llm", "lib/index.js"),
  importFromDsh("@deepseek-ai/dsh-llm-pi-ai", "lib/index.js"),
]);
const { InMemoryCredentialStore, createProvider } = piAi;
const { openAICompletionsApi } = openAiApi;
const { LlmError, assertUsableApiKey, resolveRetryPolicy } = dshLlm;
const { PiAiAdapter } = dshPiAi;

export const name = "ocg-manager-dsh";
export const inject = ["llm"];

const providerId = "ocg-manager";
const displayName = "Open Console Gateway";
const baseUrl = "http://127.0.0.1:9042/v1";
const generatedModels = "__OCG_MANAGER_GENERATED_MODELS__";

if (!Array.isArray(generatedModels)) {
  throw new Error("Open Console Gateway model catalog has not been generated.");
}

const piProvider = createProvider({
  id: providerId,
  name: displayName,
  baseUrl,
  models: generatedModels,
  api: openAICompletionsApi(),
});

export function apply(ctx) {
  const profiles = () =>
    new Map([
      [
        providerId,
        {
          provider: providerId,
          displayName,
          apiKeyEnv: "OCG_MANAGER_API_KEY",
          api: "openai-completions",
          baseURL: baseUrl,
          streamIdleTimeoutMs: 300_000,
          maxRequestImageBytes: 20 * 1024 * 1024,
          requestImagePixelBudget: 2048 * 2048,
          requestImageMaxBytes: 1024 * 1024,
          retryPolicy: resolveRetryPolicy(undefined, "ocg-manager-dsh"),
          configuredMaxTokens: new Map(),
          piProvider,
        },
      ],
    ]);

  const resolveApiKey = async (_provider, profile) => {
    const ref = profile.apiKeyEnv;
    const stored = await ctx.get("credentials")?.resolve(ref);
    const value = stored?.value ?? process.env[ref];
    if (value !== undefined && value.length > 0) {
      return assertUsableApiKey(value, name, ref);
    }
    throw new LlmError(
      `${name}: no credential stored for ${ref}`,
      "MISSING_CREDENTIAL",
    );
  };

  const adapter = new PiAiAdapter({
    profiles,
    resolveApiKey,
    auth: {
      credentials: new InMemoryCredentialStore(),
      authContext: {
        async env(variable) {
          return process.env[variable];
        },
        async fileExists() {
          return false;
        },
      },
    },
    resolveAttachments: () => ctx.get("attachments"),
  });

  ctx.llm.registerAdapter([providerId], adapter);
}
