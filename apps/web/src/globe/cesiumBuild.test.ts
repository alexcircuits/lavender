// @vitest-environment node
import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { stripCesiumDefaultToken } from '../../cesium-without-default-token';

describe('Cesium vendor account exclusion', () => {
  it('removes only the SDK initial assignment and preserves runtime access', () => {
    const transformed = stripCesiumDefaultToken(
      'const defaultAccessToken = "VENDOR_CANARY"; const Ion = {}; Ion.defaultAccessToken = defaultAccessToken; export default Ion;',
    );
    expect(transformed).not.toContain('VENDOR_CANARY');
    expect(transformed).toContain("const defaultAccessToken = '';");
    expect(transformed).toContain('Ion.defaultAccessToken = defaultAccessToken;');
    expect(transformed).toContain('export default Ion;');
  });
  it('fails closed when the installed SDK assignment changes', () => {
    expect(() => stripCesiumDefaultToken('const Ion = {};')).toThrow('source changed');
    expect(() =>
      stripCesiumDefaultToken(
        'const defaultAccessToken="a"; const defaultAccessToken="b"; Ion.defaultAccessToken=defaultAccessToken;',
      ),
    ).toThrow('source changed');
  });
  it('matches the installed SDK format without exposing the vendor credential', () => {
    const source = readFileSync(
      new URL('../../node_modules/@cesium/engine/Source/Core/Ion.js', import.meta.url),
      'utf8',
    );
    const transformed = stripCesiumDefaultToken(source);
    expect(transformed).toContain("const defaultAccessToken = '';");
    expect(transformed).not.toMatch(/eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+/);
    expect(transformed).toContain('Ion.defaultAccessToken = defaultAccessToken;');
  });
});
