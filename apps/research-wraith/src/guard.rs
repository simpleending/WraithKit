//! Network and content guards for Research Wraith: URL rules, public-address check, redirects, HTML to text.

use std::net::{IpAddr, Ipv4Addr};
use ureq::http::Uri;

/// Accepts https on port 443 to a public host name. Returns the lowercase host.
pub fn check_url(url: &str) -> Result<String, String> {
    let uri: Uri = url.parse().map_err(|_| "not a valid URL".to_string())?;
    if uri.authority().is_some_and(|a| a.as_str().contains('@')) {
        return Err("URL credentials are not allowed".into());
    }
    if uri.scheme_str() != Some("https") {
        return Err("only https URLs".into());
    }
    if uri.port_u16().is_some_and(|p| p != 443) {
        return Err("only port 443".into());
    }
    let host = uri.host().ok_or("URL has no host")?.to_ascii_lowercase();
    if host.starts_with('[')
        || host.parse::<IpAddr>().is_ok()
        || host == "localhost"
        || host.ends_with(".localhost")
        || !host.contains('.')
    {
        return Err("IP addresses and local host names are not allowed".into());
    }
    Ok(host)
}

/// The per-hop check for a fetch, a redirect target, or a fixture entry: URL rules, then the run's allowlist.
pub fn vet_hop(url: &str, allowed: &[String]) -> Result<String, String> {
    let host = check_url(url).map_err(|e| format!("blocked: {e}"))?;
    if !allowed.contains(&host) {
        return Err(format!("blocked: {host} is not on this run's source list"));
    }
    Ok(host)
}

/// True only for globally routable addresses. Loopback, private, link-local, metadata and other special ranges are refused.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let o = v.octets();
            !(v.is_private()
                || v.is_loopback()
                || v.is_link_local()
                || v.is_unspecified()
                || v.is_broadcast()
                || v.is_documentation()
                || v.is_multicast()
                || o[0] == 0
                || o[0] >= 240 // reserved, including broadcast
                || (o[0] == 100 && (o[1] & 0xc0) == 64) // carrier-grade NAT
                || (o[0] == 198 && (o[1] & 0xfe) == 18) // benchmarking
                || (o[0] == 192 && o[1] == 0 && o[2] == 0)) // IETF protocol assignments
        }
        IpAddr::V6(v) => {
            let s = v.segments();
            let v4 = |hi: u16, lo: u16| Ipv4Addr::from((u32::from(hi) << 16) | u32::from(lo));
            // IPv4 hidden inside IPv6 is judged as that IPv4: mapped or compatible (::ffff:a.b.c.d,
            // ::a.b.c.d, also ::1 -> 0.0.0.1), NAT64 (64:ff9b::/96) and 6to4 (2002::/16).
            let embedded = v.to_ipv4().or(match s {
                [0x64, 0xff9b, 0, 0, 0, 0, hi, lo] => Some(v4(hi, lo)),
                [0x2002, hi, lo, ..] => Some(v4(hi, lo)),
                _ => None,
            });
            match embedded {
                Some(m) => is_public(IpAddr::V4(m)),
                None => {
                    !(v.is_unspecified()
                        || v.is_unique_local()
                        || v.is_unicast_link_local()
                        || v.is_multicast()
                        || (s[0] == 0x2001 && s[1] == 0x0db8) // documentation
                        || (s[0] == 0x2001 && s[1] == 0)) // Teredo, which hides an IPv4 address
                }
            }
        }
    }
}

/// Decides the next URL after a response. `Ok(None)` means the response is final.
/// Absolute https and absolute-path redirects are accepted. The caller must run `vet_hop` on the result.
pub fn next_hop(
    current: &str,
    status: u16,
    location: Option<&str>,
) -> Result<Option<String>, String> {
    if !(300..400).contains(&status) {
        return Ok(None);
    }
    let loc = location.ok_or("blocked: redirect without a Location header")?;
    if loc.starts_with("https://") {
        return Ok(Some(loc.to_string()));
    }
    if !loc.starts_with('/') || loc.starts_with("//") {
        return Err(format!("blocked: unsupported redirect target {loc}"));
    }
    let uri: Uri = current
        .parse()
        .map_err(|_| "blocked: bad URL in redirect".to_string())?;
    let host = uri
        .host()
        .ok_or("blocked: redirect from a URL with no host")?;
    Ok(Some(format!("https://{host}{loc}")))
}

/// The message for a rejected brief name. It quotes the name and gives an allowed one, so a model can correct itself in one turn.
pub fn brief_name_error(name: &str) -> String {
    let shown: String = name.chars().take(60).collect();
    format!(
        "brief name {shown:?} is not allowed: use 1 to 40 characters of a-z, 0-9 and hyphens only, no spaces, dots, underscores or capitals, for example rfc2606-domains"
    )
}

/// A file name for a brief: 1 to 40 characters of a-z, 0-9 and hyphen.
pub fn valid_brief_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 40
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Drops tags, script and style bodies, decodes common entities, and collapses whitespace.
/// Hidden HTML (for example `display:none`) is kept on purpose. The model sees it, and the evidence flag catches it.
pub fn html_to_text(html: &str) -> String {
    let lower = html.to_ascii_lowercase(); // same byte offsets as `html`
    let (mut out, mut i) = (String::new(), 0);
    while let Some(rel) = html[i..].find('<') {
        out.push_str(&html[i..i + rel]);
        let tag = i + rel;
        let skip_to = ["script", "style"]
            .iter()
            .find(|t| lower[tag + 1..].starts_with(*t))
            .and_then(|t| lower[tag..].find(&format!("</{t}")).map(|e| tag + e))
            .unwrap_or(tag);
        i = lower[skip_to..]
            .find('>')
            .map_or(html.len(), |e| skip_to + e + 1);
        out.push(' ');
    }
    out.push_str(&html[i..]);
    let out = out
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        assert_eq!(check_url("https://ok.example/path").unwrap(), "ok.example");
        assert_eq!(check_url("https://OK.example:443/x").unwrap(), "ok.example");
        for bad in [
            "http://ok.example/",
            "https://ok.example:8443/",
            "https://127.0.0.1/",
            "https://[::1]/",
            "https://localhost/",
            "https://app.localhost/",
            "https://intranet/",
            "not a url",
        ] {
            assert!(check_url(bad).is_err(), "{bad} must be refused");
        }
    }

    #[test]
    fn ips() {
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "::1",
            "::",
            "::ffff:127.0.0.1",
            "fc00::1",
            "fe80::1",
            "224.0.0.1",
            // reserved IPv4 ranges
            "240.0.0.1",
            "255.255.255.255",
            "198.18.0.1",
            "192.0.0.8",
            // IPv4 hidden inside IPv6: compatible, NAT64, 6to4, Teredo, documentation
            "::169.254.169.254",
            "64:ff9b::a9fe:a9fe",
            "2002:a9fe:a9fe::1",
            "2001:0:4136:e378::1",
            "2001:db8::1",
        ] {
            assert!(!is_public(ip.parse().unwrap()), "{ip} must not be public");
        }
        for ip in [
            "8.8.8.8",
            "1.1.1.1",
            "2606:4700::1111",
            "64:ff9b::808:808",
            "2002:808:808::1",
        ] {
            assert!(is_public(ip.parse().unwrap()), "{ip} is public");
        }
    }

    #[test]
    fn html() {
        assert_eq!(
            html_to_text("<p>a &amp; b</p><script>x=1</script><style>p{}</style>c"),
            "a & b c"
        );
        assert_eq!(
            html_to_text("<p>Submissions close   December 4, 2026</p>"),
            "Submissions close December 4, 2026"
        );
        assert_eq!(
            html_to_text("<div style=\"display:none\">hidden &lt;note&gt;</div>"),
            "hidden <note>"
        );
        assert_eq!(
            html_to_text("plain &#39;text&#39;&nbsp;here"),
            "plain 'text' here"
        );
    }

    #[test]
    fn redirect_rechecked() {
        let allowed = vec!["a.example".to_string()];
        let next = next_hop("https://a.example/x", 302, Some("https://evil.example/")).unwrap();
        assert_eq!(next.as_deref(), Some("https://evil.example/"));
        let refused = vet_hop(&next.unwrap(), &allowed).unwrap_err();
        assert!(
            refused.starts_with("blocked:") && refused.contains("evil.example"),
            "{refused}"
        );
        assert_eq!(
            next_hop("https://a.example/x", 302, Some("/y"))
                .unwrap()
                .as_deref(),
            Some("https://a.example/y")
        );
        let plain = next_hop("https://a.example/x", 302, Some("http://a.example/")).unwrap_err();
        assert!(
            plain.starts_with("blocked:"),
            "an http redirect is refused outright: {plain}"
        );
        assert!(
            vet_hop("http://a.example/", &allowed).is_err(),
            "a plain http hop fails the URL rules too"
        );
        assert!(next_hop("https://a.example/x", 302, Some("//evil.example/")).is_err());
        assert!(next_hop("https://a.example/x", 302, None).is_err());
        assert_eq!(next_hop("https://a.example/x", 200, None).unwrap(), None);
    }

    #[test]
    fn brief_name_rules() {
        assert!(valid_brief_name("hollow-hacks-deadline"));
        assert!(valid_brief_name("a1"));
        assert!(!valid_brief_name(""));
        assert!(!valid_brief_name("Upper"));
        assert!(!valid_brief_name("../escape"));
        assert!(!valid_brief_name("with space"));
        assert!(!valid_brief_name(&"a".repeat(41)));
        assert!(valid_brief_name(&"a".repeat(40)));
    }
}
