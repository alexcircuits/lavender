/**
 * Links that come from providers are untrusted. Lavender renders a link only for an absolute
 * `https:` URL without embedded credentials; anything else is shown as text or not at all.
 */

/** The normalised URL when `raw` is a safe https link, else null. */
export function safeHttpsUrl(raw: string | null | undefined): string | null {
  if (typeof raw !== 'string' || raw.trim() === '') return null;
  let url: URL;
  try {
    url = new URL(raw.trim());
  } catch {
    return null;
  }
  if (url.protocol !== 'https:' || url.username !== '' || url.password !== '') return null;
  return url.href;
}

/** A safe https link on `usgs.gov` or one of its subdomains (an earthquake's event page). */
export function usgsEventUrl(raw: string | null | undefined): string | null {
  const href = safeHttpsUrl(raw);
  if (!href) return null;
  const host = new URL(href).hostname.toLowerCase();
  return host === 'usgs.gov' || host.endsWith('.usgs.gov') ? href : null;
}
