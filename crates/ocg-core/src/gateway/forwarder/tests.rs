use super::*;

#[test]
fn explicit_opencode_identity_is_copied_from_the_original_client_map() {
    let mut client = HeaderMap::new();
    client.insert("x-opencode-client", "desktop".parse().unwrap());
    client.insert("x-opencode-request", "req_keep".parse().unwrap());
    client.insert("x-opencode-project", "proj_keep".parse().unwrap());
    client.insert("x-session-id", "ses_not_identity".parse().unwrap());
    let mut upstream = reqwest::header::HeaderMap::new();
    copy_explicit_opencode_identity_headers(&mut upstream, &client);
    assert_eq!(upstream.get("x-opencode-client").unwrap(), "desktop");
    assert_eq!(upstream.get("x-opencode-request").unwrap(), "req_keep");
    assert_eq!(upstream.get("x-opencode-project").unwrap(), "proj_keep");
    assert!(upstream.get("x-session-id").is_none());
    assert!(upstream.get("x-opencode-session").is_none());
}
