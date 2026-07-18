const NETWORK_ERROR_PATTERN = /failed to fetch|network(?:error| request failed)|load failed/i;

export function userFacingError(error: unknown, networkFallback: string): string {
  if (error instanceof TypeError && NETWORK_ERROR_PATTERN.test(error.message)) {
    return networkFallback;
  }
  return error instanceof Error ? error.message : String(error);
}
