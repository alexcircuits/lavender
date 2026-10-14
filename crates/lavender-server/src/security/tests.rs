//! Regression tests for the parent module.

use super::*;

fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
    let mut h = HeaderMap::new();
    for (k, v) in pairs {
        h.append(*k, HeaderValue::from_static(v));
    }
    h
}

fn origin_path() -> Uri {
    Uri::from_static("/api/v1/status")
}

#[test]
fn host_names_strip_ports() {
    assert_eq!(host_name("localhost:7410"), "localhost");
    assert_eq!(host_name("[::1]:7410"), "[::1]");
    assert_eq!(host_name("example.com"), "example.com");
    assert_eq!(host_name("user@example.com:80"), "example.com");
}

#[test]
fn host_policy_reads_the_request_target_when_there_is_no_host_header() {
    // Review finding 12: HTTP/2 (`:authority`) and absolute-form requests were refused
    // because only the Host header was read.
    let policy = HostPolicy::new(&["localhost".to_owned()]);
    let absolute = Uri::from_static("http://localhost:7410/api/v1/status");
    assert!(policy.allows(&HeaderMap::new(), &absolute));
    let foreign = Uri::from_static("http://evil.example/api/v1/status");
    assert!(!policy.allows(&HeaderMap::new(), &foreign));
    assert!(
        !policy.allows(&headers(&[("host", "localhost")]), &foreign),
        "a Host that disagrees with the target is refused"
    );
    assert!(policy.allows(&headers(&[("host", "localhost:7410")]), &origin_path()));
    assert!(!policy.allows(&HeaderMap::new(), &origin_path()), "no host at all");
}

#[test]
fn same_origin_rules() {
    let uri = origin_path();
    assert!(is_same_origin(&headers(&[("host", "localhost:7410")]), &uri));
    assert!(is_same_origin(
        &headers(&[("host", "localhost:7410"), ("origin", "http://localhost:7410")]),
        &uri
    ));
    assert!(!is_same_origin(
        &headers(&[("host", "localhost:7410"), ("origin", "https://evil.example")]),
        &uri
    ));
    assert!(!is_same_origin(
        &headers(&[("host", "localhost:7410"), ("sec-fetch-site", "cross-site")]),
        &uri
    ));
    assert!(!is_same_origin(&headers(&[("host", "localhost:7410"), ("origin", "null")]), &uri));
}

#[test]
fn same_origin_checks_the_scheme() {
    // Review LOW: the scheme was ignored, so an http:// page could call an https site.
    let uri = origin_path();
    let behind_tls = |origin: &'static str| {
        headers(&[("host", "lavender.example"), ("x-forwarded-proto", "https"), ("origin", origin)])
    };
    assert!(is_same_origin(&behind_tls("https://lavender.example"), &uri));
    assert!(!is_same_origin(&behind_tls("http://lavender.example"), &uri));
    assert!(
        is_same_origin(
            &headers(&[("host", "lavender.example"), ("origin", "https://lavender.example")]),
            &uri
        ),
        "a TLS proxy that does not send X-Forwarded-Proto still works"
    );
    assert!(!is_same_origin(
        &headers(&[("host", "localhost:7410"), ("origin", "ftp://localhost:7410")]),
        &uri
    ));
    let h2 = Uri::from_static("https://lavender.example/api/v1/status");
    assert!(is_same_origin(&headers(&[("origin", "https://lavender.example")]), &h2));
    assert!(!is_same_origin(&headers(&[("origin", "http://lavender.example")]), &h2));
}

#[test]
fn bucket_limits_bursts_and_refills() {
    let limiter = RateLimiter::new(1, 2, vec![]);
    let ip = IpAddr::from([10, 0, 0, 1]);
    assert!(limiter.take(ip).is_ok());
    assert!(limiter.take(ip).is_ok());
    assert_eq!(limiter.take(ip), Err(1));
    assert!(limiter.take(IpAddr::from([10, 0, 0, 2])).is_ok(), "clients are independent");
}

#[test]
fn forwarded_for_is_read_right_to_left_from_trusted_proxies_only() {
    // Review finding 2: the leftmost entry is written by the client, so trusting it let
    // a client rotate fake addresses past the rate limit.
    let proxy = IpAddr::from([10, 0, 0, 9]);
    let limiter = RateLimiter::new(1, 1, vec![proxy, IpAddr::from([10, 0, 0, 8])]);
    let real = IpAddr::from([203, 0, 113, 7]);
    // The client claims 1.1.1.1; the proxy appended the address it really saw.
    let spoofed = headers(&[("x-forwarded-for", "1.1.1.1, 203.0.113.7")]);
    assert_eq!(limiter.client(proxy, &spoofed), real);
    // Two trusted proxies in a chain are skipped.
    let chain = headers(&[("x-forwarded-for", "9.9.9.9, 203.0.113.7, 10.0.0.8")]);
    assert_eq!(limiter.client(proxy, &chain), real);
    // Repeated headers count as one list, in order.
    let split = headers(&[("x-forwarded-for", "9.9.9.9"), ("x-forwarded-for", "203.0.113.7")]);
    assert_eq!(limiter.client(proxy, &split), real);
    // Ports and IPv4-mapped peers are normalised.
    let ported = headers(&[("x-forwarded-for", "203.0.113.7:51234")]);
    assert_eq!(limiter.client(IpAddr::from([0, 0, 0, 0, 0, 0xffff, 0x0a00, 9]), &ported), real);
    // Garbage stops the walk at the nearest trusted hop.
    let garbage = headers(&[("x-forwarded-for", "203.0.113.7, not-an-ip")]);
    assert_eq!(limiter.client(proxy, &garbage), proxy);
    // Untrusted peers are never overridden.
    let direct = IpAddr::from([1, 2, 3, 4]);
    assert_eq!(limiter.client(direct, &spoofed), direct);
}

#[test]
fn rotating_forwarded_for_does_not_escape_the_limit() {
    let proxy = IpAddr::from([10, 0, 0, 9]);
    let limiter = RateLimiter::new(1, 3, vec![proxy]);
    let allowed = (0..50u8)
        .filter(|i| {
            let value = format!("198.51.100.{i}, 203.0.113.7");
            let mut h = HeaderMap::new();
            h.insert("x-forwarded-for", HeaderValue::from_str(&value).unwrap());
            limiter.take(limiter.client(proxy, &h)).is_ok()
        })
        .count();
    assert_eq!(allowed, 3, "only the burst passes");
}

#[test]
fn ipv6_clients_are_keyed_by_their_slash_64() {
    // Review finding 6: per-address IPv6 keys let one /64 rotate through fresh buckets.
    let a: IpAddr = "2001:db8:1:2::1".parse().unwrap();
    let b: IpAddr = "2001:db8:1:2:ffff::9".parse().unwrap();
    let other: IpAddr = "2001:db8:1:3::1".parse().unwrap();
    assert_eq!(client_key(a), client_key(b));
    assert_ne!(client_key(a), client_key(other));
    let mapped: IpAddr = "::ffff:192.0.2.1".parse().unwrap();
    assert_eq!(client_key(mapped), IpAddr::from([192, 0, 2, 1]));
    let limiter = RateLimiter::new(1, 2, vec![]);
    assert!(limiter.take(a).is_ok() && limiter.take(b).is_ok());
    assert_eq!(limiter.take("2001:db8:1:2::77".parse().unwrap()), Err(1));
}

#[test]
fn the_bucket_table_is_bounded() {
    // Review finding 6: buckets were evicted only after 300 s idle, so the table grew
    // without bound and every request past 10 000 clients paid an O(n) sweep.
    let limiter = RateLimiter::with_capacity(1, 5, vec![], 1_000);
    for i in 0..20_000u32 {
        let _ = limiter.take(IpAddr::from(i.to_be_bytes()));
    }
    limiter.buckets.run_pending_tasks();
    assert!(limiter.buckets.entry_count() <= 1_000, "{}", limiter.buckets.entry_count());
}

#[test]
fn csp_has_no_unsafe_eval_and_lists_tile_hosts() {
    let csp = content_security_policy(&["gibs.earthdata.nasa.gov".to_owned()]);
    assert!(!csp.contains("'unsafe-eval'"));
    assert!(csp.contains("'wasm-unsafe-eval'"));
    assert!(csp.contains("img-src 'self' data: blob: https://gibs.earthdata.nasa.gov"));
    assert!(csp.contains("frame-ancestors 'none'"));
}
