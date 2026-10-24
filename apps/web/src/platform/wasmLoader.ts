/**
 * Platform helper for fetching WebAssembly binary assets (ADR 0005).
 */
export async function fetchWasmResponse(url: string): Promise<Response> {
  return fetch(url);
}
