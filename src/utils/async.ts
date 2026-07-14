/**
 * Run an async worker over every item with a bounded concurrency limit.
 *
 * Useful when an operation fans out into many per-entity requests (e.g.
 * loading usage for each account) and unbounded parallelism would hammer the
 * backend or trip rate limits. Order of results matches the input order.
 *
 * Returns a list of settled results so callers can filter fulfilled/rejected
 * without an extra try/catch per item.
 */
export async function mapWithConcurrency<T, R>(
  items: readonly T[],
  limit: number,
  worker: (item: T, index: number) => Promise<R>,
): Promise<PromiseSettledResult<R>[]> {
  const results: PromiseSettledResult<R>[] = new Array(items.length);
  let cursor = 0;
  async function run(): Promise<void> {
    while (cursor < items.length) {
      const index = cursor++;
      try {
        results[index] = { status: "fulfilled", value: await worker(items[index], index) };
      } catch (error) {
        results[index] = { status: "rejected", reason: error };
      }
    }
  }
  const runners = Array.from({ length: Math.min(limit, items.length) }, run);
  await Promise.all(runners);
  return results;
}
