import type { Plugin } from 'vite';

/** Lavender never uses the SDK's vendor account; keep its unused credential out of dist. */
export function stripCesiumDefaultToken(source: string): string {
  const assignment = /Ion\.defaultAccessToken\s*=\s*["'][^"']+["']\s*;/g;
  const matches = [...source.matchAll(assignment)];
  if (matches.length !== 1)
    throw new Error('Cesium Ion default-token source changed; review the SDK transform.');
  return source.replace(assignment, "Ion.defaultAccessToken = '';");
}

export function cesiumWithoutDefaultToken(): Plugin {
  return {
    name: 'lavender-cesium-without-default-token',
    enforce: 'pre',
    transform(source, id) {
      if (!id.replaceAll('\\', '/').endsWith('/@cesium/engine/Source/Core/Ion.js')) return null;
      return { code: stripCesiumDefaultToken(source), map: null };
    },
  };
}
