import { describe, expect, it } from 'vitest';
import { stripCesiumDefaultToken } from '../cesium-without-default-token';

describe('Cesium vendor account exclusion', () => {
  it('removes only the SDK initial assignment and preserves runtime access', () => {
    const transformed = stripCesiumDefaultToken(
      'const Ion = {}; Ion.defaultAccessToken = "VENDOR_CANARY"; export default Ion;',
    );
    expect(transformed).not.toContain('VENDOR_CANARY');
    expect(transformed).toContain("Ion.defaultAccessToken = '';");
    expect(transformed).toContain('export default Ion;');
  });
  it('fails closed when the installed SDK assignment changes', () => {
    expect(() => stripCesiumDefaultToken('const Ion = {};')).toThrow('source changed');
    expect(() =>
      stripCesiumDefaultToken('Ion.defaultAccessToken="a"; Ion.defaultAccessToken="b";'),
    ).toThrow('source changed');
  });
});
