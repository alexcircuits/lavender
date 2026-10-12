/**
 * Table lookups keyed by values that come from outside (server enums, provider strings).
 * A server newer than this client can send a value the table does not know, and a provider can
 * send "constructor"; both must fall back, never throw or return an inherited property.
 */
export function lookup<V>(table: Readonly<Record<string, V>>, key: string, fallback: V): V {
  return Object.hasOwn(table, key) ? (table[key] as V) : fallback;
}
