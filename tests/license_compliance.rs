// Safeguards consistency between the published license, Cargo metadata and attribution.
#[test]
fn repository_has_complete_apache_2_license() {
    let text = include_str!("../LICENSE");
    assert!(text.contains("Apache License"));
    assert!(text.contains("Version 2.0, January 2004"));
    assert!(text.contains("END OF TERMS AND CONDITIONS"));
    assert!(text.contains("APPENDIX: How to apply the Apache License"));
}

#[test]
fn cargo_and_notice_match_apache_license() {
    assert_eq!(env!("CARGO_PKG_LICENSE"), "Apache-2.0");
    let notice = include_str!("../NOTICE");
    assert!(notice.contains("Copyright 2026 RudyCity"));
    assert!(notice.contains("Apache License, Version 2.0"));
}
