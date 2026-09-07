use super::*;
use axum::http::HeaderValue;

#[test]
fn cookie_is_httponly_strict_and_scoped_to_dashboard() {
    let headers = HeaderMap::new();
    assert_eq!(
        cookie_header("abc123", &headers, false)
            .unwrap()
            .to_str()
            .unwrap(),
        "ocg_dashboard_session=abc123; HttpOnly; SameSite=Strict; Path=/dashboard"
    );
}

#[test]
fn cleared_cookie_sets_max_age_zero_and_empty_value() {
    let headers = HeaderMap::new();
    assert_eq!(
        cookie_header("", &headers, true).unwrap().to_str().unwrap(),
        "ocg_dashboard_session=; HttpOnly; SameSite=Strict; Path=/dashboard; Max-Age=0"
    );
}

#[test]
fn secure_is_inferred_only_from_https_forwarded_proto() {
    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-proto", HeaderValue::from_static("https"));
    assert!(
        cookie_header("tok", &headers, false)
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with("; Secure")
    );

    headers.insert("x-forwarded-proto", HeaderValue::from_static("HTTPS"));
    assert!(
        cookie_header("tok", &headers, false)
            .unwrap()
            .to_str()
            .unwrap()
            .contains("; Secure")
    );

    headers.insert("x-forwarded-proto", HeaderValue::from_static("http"));
    assert!(
        !cookie_header("tok", &headers, false)
            .unwrap()
            .to_str()
            .unwrap()
            .contains("Secure")
    );
}

#[test]
fn loopback_trust_requires_local_mode_and_no_forwarded_headers() {
    let mut headers = HeaderMap::new();
    assert!(!is_local_dashboard_request(true, &headers));
    headers.insert(header::HOST, HeaderValue::from_static("127.0.0.1:9042"));
    assert!(is_local_dashboard_request(true, &headers));
    assert!(!is_local_dashboard_request(false, &headers));
    headers.insert("x-forwarded-for", HeaderValue::from_static("203.0.113.10"));
    assert!(!is_local_dashboard_request(true, &headers));
}

#[test]
fn session_cookie_matches_the_current_token_exactly() {
    let mut headers = HeaderMap::new();
    headers.insert(header::HOST, HeaderValue::from_static("localhost:9042"));
    headers.insert(
        header::COOKIE,
        HeaderValue::from_static("theme=dark; ocg_dashboard_session=abc123"),
    );
    assert!(has_dashboard_session("abc123", &headers));
    assert!(!has_dashboard_session("other", &headers));
    assert!(is_authorized(false, "abc123", &headers));
    assert!(!is_authorized(false, "other", &headers));
    assert!(is_authorized(true, "other", &headers));
}

#[test]
fn local_bypass_checks_authority_origin_and_fetch_metadata() {
    for host in [
        "127.0.0.1:9042",
        "127.0.0.2:9042",
        "localhost:30001",
        "LOCALHOST:9042",
        "[::1]:9042",
    ] {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, host.parse().unwrap());
        headers.insert(header::ORIGIN, format!("http://{host}").parse().unwrap());
        assert!(is_local_dashboard_request(true, &headers), "{host}");
    }
    for host in [
        "attacker.invalid:9042",
        "localhost.attacker.invalid",
        "127.0.0.1.attacker.invalid",
        "192.168.0.1",
        "user@localhost:9042",
        "localhost:bad",
        "localhost:",
        "localhost:99999",
        "localhost,attacker.invalid",
    ] {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, host.parse().unwrap());
        assert!(!is_local_dashboard_request(true, &headers), "{host}");
    }
    let mut headers = HeaderMap::new();
    headers.insert(header::HOST, "127.0.0.1:9042".parse().unwrap());
    for origin in [
        "null",
        "https://attacker.invalid",
        "http://127.0.0.1:9043",
        "http://localhost:9042",
        "http://user@127.0.0.1:9042",
        "http://127.0.0.1:9042/path",
        "http://127.0.0.1:9042?x=1",
        "http://127.0.0.1:9042#fragment",
        "http://127.0.0.1:9042 http://attacker.invalid",
    ] {
        headers.insert(header::ORIGIN, origin.parse().unwrap());
        assert!(!is_local_dashboard_request(true, &headers), "{origin}");
    }
    headers.insert(header::ORIGIN, "http://127.0.0.1:9042".parse().unwrap());
    headers.append(header::ORIGIN, "http://127.0.0.1:9042".parse().unwrap());
    assert!(!is_local_dashboard_request(true, &headers));
    headers.remove(header::ORIGIN);
    headers.insert("sec-fetch-site", "cross-site".parse().unwrap());
    assert!(!is_local_dashboard_request(true, &headers));
    headers.remove("sec-fetch-site");
    headers.append(header::HOST, "127.0.0.1:9042".parse().unwrap());
    assert!(!is_local_dashboard_request(true, &headers));
}
