// Proxies aetower.dev release paths to the aetower-releases Pages project.
//
// Pure passthrough for bodies: bytes are streamed exactly as Pages serves
// them (Sparkle EdDSA signatures are over exact bytes; never transform).
//
// Range and conditional handling: the *.pages.dev origin ignores Range
// headers (200 + full body) and Workers subrequests to it bypass the edge
// fetch-cache, so this worker fronts the origin with the Cache API instead.
// caches.default.match() natively serves 206 for ranged requests and 304 for
// If-None-Match/If-Modified-Since from a stored full response — restoring
// the behavior the zone cache provided before the site/payload split
// (Sparkle download resume relies on 206). Entries expire per the payload's
// Cache-Control (immutable for versioned artifacts, 120 s for aliases), and
// only 200s are ever stored, so errors are never cached.
//
// Note: the Cache API is a no-op on the workers.dev preview host — there,
// every request takes the direct-fetch fallback path, which is still a
// correct (rangeless) proxy. 206/304 behavior is only observable on the
// aetower.dev routes.
const ORIGIN = "https://aetower-releases.pages.dev";

export default {
  async fetch(request, _env, ctx) {
    const url = new URL(request.url);
    const originUrl = ORIGIN + url.pathname + url.search;
    const cache = caches.default;

    if (request.method === "GET") {
      const cached = await cache.match(request);
      if (cached) {
        return withMarker(cached);
      }
    }

    // Fetch the FULL object: strip Range/conditional headers so a complete
    // 200 body is available to store; the cache serves the 206/304 variants.
    const originHeaders = new Headers(request.headers);
    originHeaders.delete("range");
    originHeaders.delete("if-none-match");
    originHeaders.delete("if-modified-since");

    let response;
    try {
      response = await fetch(originUrl, {
        method: request.method,
        headers: originHeaders,
        redirect: "follow", // absorb Pages-internal redirects; never leak a pages.dev Location
      });
    } catch (_err) {
      return new Response("release origin unavailable\n", {
        status: 502,
        headers: { "cache-control": "no-store" },
      });
    }

    if (request.method === "GET" && response.status === 200) {
      // Key by the INCOMING url (a bare GET) so the match() lookups above —
      // which use the incoming request and its Range/conditional headers —
      // hit this entry.
      await cache.put(new Request(request.url), response.clone());
      // Re-match with the original request so Range/If-None-Match are
      // honored (206/304) even on the first, cache-cold request.
      const stored = await cache.match(request);
      if (stored) {
        return withMarker(stored);
      }
    }

    return withMarker(response);
  },
};

function withMarker(response) {
  // fetch()/cache.match() responses have immutable headers; re-wrap (the
  // body still streams) to add the marker the deploy verification checks.
  const out = new Response(response.body, response);
  out.headers.set("x-aetower-release-router", "1");
  return out;
}
