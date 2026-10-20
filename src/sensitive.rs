// SPDX-License-Identifier: Apache-2.0

//! Detect clips that contain a known secret shape.
//!
//! Ported from Clipman's `clipman/sensitive.py` (Apache-2.0, see NOTICE).
//! A flagged clip is masked and then deleted after the sensitive timeout, so a
//! false positive means data loss while a false negative only leaves a secret
//! in the local history, which the 0700 data directory already protects. The
//! detector therefore only matches known shapes and never guesses from
//! randomness or character classes.
//!
//! Python's `re` features that Rust's `regex` crate does not have — lookaround
//! — are replaced by explicit boundary checks, and every class is written in
//! ASCII form because Python compiled these patterns with `re.ASCII`.

use std::sync::LazyLock;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE;
use regex::Regex;

/// Only the first 64 KiB is scanned; the cost is about 0.5 ms per KB.
pub const SCAN_LIMIT: usize = 64 * 1024;

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("sensitive pattern compiles")
}

/// Vendor tokens: a namespaced prefix plus the documented alphabet. Length
/// floors sit below the documented lengths; vendors lengthen them.
const VENDOR_PATTERNS: &[&str] = &[
    // GitHub classic and fine-grained tokens.
    r"gh[pousr]_[A-Za-z0-9]{12,}",
    r"github_pat_[A-Za-z0-9_]{16,}",
    // AWS access key ids: type prefix, then 16 chars with a digit. The digit
    // check runs in `has_aws_access_key`; Rust's regex has no lookaround.
    // Stripe secret, restricted and webhook keys. Publishable keys (pk_) ship
    // in web pages; they are not secrets.
    r"(?:sk|rk)_(?:live|test)_[A-Za-z0-9]{16,}",
    r"whsec_[A-Za-z0-9]{24,}",
    // Slack tokens, app tokens and incoming webhooks.
    r"xox[abopers]-[A-Za-z0-9-]{10,}",
    r"xapp-[0-9]-[A-Z0-9]+-[0-9]+-[a-z0-9]{20,}",
    r"hooks\.slack\.com/services/T[A-Z0-9]{6,}/B[A-Z0-9]{6,}/[A-Za-z0-9]{16,}",
    // Google API keys, OAuth access and refresh tokens, client secrets.
    r"AIza[0-9A-Za-z_-]{15,}",
    r"ya29\.[0-9A-Za-z_-]{20,}",
    r"1//0[0-9A-Za-z_-]{30,}",
    r"GOCSPX-[0-9A-Za-z_-]{20,}",
    // npm, PyPI (fixed macaroon location prefix) and Hugging Face.
    r"npm_[A-Za-z0-9]{16,}",
    r"pypi-AgEIcHlwaS5vcmc[A-Za-z0-9_-]{30,}",
    r"hf_[A-Za-z0-9]{30,}",
    // Twilio API key SID; account SID only when paired with its token.
    r"SK[0-9a-f]{32}",
    r"AC[0-9a-f]{32}:[0-9a-f]{32}",
    // SendGrid.
    r"SG\.[A-Za-z0-9_-]{16,32}\.[A-Za-z0-9_-]{32,64}",
    // Azure AD client secrets carry a fixed "7Q~" or "8Q~" marker.
    r"[A-Za-z0-9_.~-]{3}[78]Q~[A-Za-z0-9_.~-]{31,34}",
    // DigitalOcean, GitLab, Shopify, Square, Linear, Figma, Docker Hub.
    r"do[opr]_v1_[a-f0-9]{64}",
    r"glpat-[A-Za-z0-9_-]{20,}",
    r"shp(?:at|ss|ca|pa)_[a-fA-F0-9]{32}",
    r"sq0(?:atp|csp)-[A-Za-z0-9_-]{22,}",
    r"lin_api_[A-Za-z0-9]{32,}",
    r"figd_[A-Za-z0-9_-]{32,}",
    r"dckr_pat_[A-Za-z0-9_-]{20,}",
    // Facebook, Telegram bot, Discord bot, Mailchimp.
    r"EAACEdEose0cBA[A-Za-z0-9]{20,}",
    r"[0-9]{8,10}:AA[A-Za-z0-9_-]{33}",
    r"[MN][A-Za-z0-9_-]{23,25}\.[A-Za-z0-9_-]{6}\.[A-Za-z0-9_-]{27,40}",
    r"[a-f0-9]{32}-us[0-9]{1,2}",
    // age identities.
    r"AGE-SECRET-KEY-1[A-Z0-9]{58}",
    // Discord webhooks: whoever has the URL can post as the webhook.
    r"discord(?:app)?\.com/api/webhooks/[0-9]{16,20}/[A-Za-z0-9_-]{60,}",
];

/// "sk-" is shared by OpenAI-style keys (sk-..., sk-proj-..., sk-ant-...) and
/// CSS class names or names such as "sk-fading-circle" or "sk-prod-cluster-01".
/// Candidates are whole tokens, found in linear time; `has_sk_key` then checks
/// the body: long, with a letter, a digit and an unbroken random-looking run.
const SK_MAX_TOKEN: usize = 256;

/// Labelled secrets: the keyword ends the key and "=" or ":" follows.
const KEYWORDS: &str = r"password|passwd|passphrase|passcode|pwd|secret|token|apikey|(?:api|private|access|account|secret|app|signing|encryption|master|auth)[_-]?key";

const IDENT_CHARS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_.-";
const LINE_LEAD_CHARS: &str = " \t\"'{,[-";

/// Values that are placeholders rather than literal secrets.
const PLACEHOLDER_WORDS: &[&str] = &[
    "your",
    "example",
    "changeme",
    "change_me",
    "change-me",
    "replace",
    "placeholder",
    "redacted",
    "dummy",
    "sample",
    "insert",
    "xxxx",
    "****",
    "todo",
    "here",
];
const VALUE_BAD_LEAD: &str = "$<{[%(~/=*.";
const CODE_CHARS: &[char] = &['(', ')', '[', ']'];

const CLI_MAX_LINE: usize = 1024;

static VENDOR: LazyLock<Vec<Regex>> =
    LazyLock::new(|| VENDOR_PATTERNS.iter().map(|p| re(p)).collect());
static SK_CANDIDATE: LazyLock<Regex> = LazyLock::new(|| re(r"sk-[A-Za-z0-9_-]{32,}"));
static AWS_KEY: LazyLock<Regex> = LazyLock::new(|| re(r"(?:AKIA|ASIA|ABIA|ACCA)[A-Z0-9]{16}"));
static SK_RUN: LazyLock<Regex> = LazyLock::new(|| re(r"[A-Za-z0-9]{16}"));
static PRIVATE_KEY: LazyLock<Regex> = LazyLock::new(|| {
    re(r"-----BEGIN [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----|PuTTY-User-Key-File-[0-9]")
});
static CRED_URL: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"[A-Za-z][A-Za-z0-9+.-]{1,31}://[^ \t\n\r/:@\x0b\x0c]{0,128}:[^ \t\n\r/@\x0b\x0c]{1,256}@[^ \t\n\r/@\x0b\x0c]+",
    )
});
/// The same authority without a scheme, as a whole single-line clip. Only
/// numeric or localhost hosts qualify; a DNS name looks like mailto.
static BARE_CRED: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"\A[A-Za-z0-9._-]{1,64}:[^ \t\n\r@/\x0b\x0c]{6,}@(?:(?:[0-9]{1,3}\.){3}[0-9]{1,3}|localhost)(?::[0-9]{1,5})?(?:/[^ \t\n\r\x0b\x0c]*)?\z",
    )
});
static JWT: LazyLock<Regex> = LazyLock::new(|| re(r"eyJ[A-Za-z0-9_-]{8,}"));
static LABEL: LazyLock<Regex> = LazyLock::new(|| {
    re(&format!(
        r#"(?i:{KEYWORDS})["']?[ \t]*([=:])[ \t]*["']?([^ \t\n\r"';,\x0b\x0c]{{8,}})"#
    ))
});
/// `DB_PASS`, `SMTP_PASS`: "pass" only as a suffix, never in "bypass".
static LABEL_PASS_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    re(r#"(?i)[A-Za-z0-9][_-](pass)["']?[ \t]*([=:])[ \t]*["']?([^ \t\n\r"';,\x0b\x0c]{8,})"#)
});
static DOTTED_NAME: LazyLock<Regex> =
    LazyLock::new(|| re(r"[A-Za-z_][0-9A-Za-z_]*(?:\.[A-Za-z_][0-9A-Za-z_]*)+"));
static AT_NAME: LazyLock<Regex> = LazyLock::new(|| re(r"@[A-Za-z_][0-9A-Za-z_-]*"));
/// HTTP Authorization headers and bare Bearer tokens.
static AUTH_HEADER: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r#"(?i)(?:proxy-)?authorization["']?[ \t]*:[ \t]*["']?(?:(?:bearer|basic|digest|token|negotiate|hmac|oauth|apikey|api-key)[ \t]+)?([^ \t\n\r"',\x0b\x0c]{8,})"#,
    )
});
static BEARER: LazyLock<Regex> = LazyLock::new(|| re(r#"Bearer[ \t]+([^ \t\n\r"',\x0b\x0c]{8,})"#));
/// TOTP seeds: an otpauth URI or a labelled base32 seed, maybe in groups.
static OTPAUTH: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)otpauth://[ht]otp/"));
static TOTP_LABEL: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r#"(?i)(?:(?:totp|otp|mfa|2fa)[_-]?)?secret["']?[ \t]*[:=][ \t]*["']?((?:[A-Z2-7]{4}[ -]){3,}[A-Z2-7]{4,}|[A-Z2-7]{16,})"#,
    )
});
/// Command lines that take a password inline. They are only looked for in lines
/// of command length, and the gap between the command and its flag is bounded:
/// a long line full of "http" or "mysql" used to cost seconds.
static CLI_DASH_P: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r#"(mysql|mysqldump|mysqladmin|mariadb|sshpass)[^\n]{0,300}?[ \t]-p([^ \t\n\r"'\x0b\x0c]{6,})"#,
    )
});
static SSHPASS_P: LazyLock<Regex> =
    LazyLock::new(|| re(r#"sshpass[^\n]{0,300}?[ \t]-p[ \t]+["']?([^ \t\n\r"'\x0b\x0c]{4,})"#));
static CLI_USER_PASS: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r#"(curl|wget|http|https)[^\n]{0,300}?[ \t](?:-u|--user|-a|--auth)[ \t=]+["']?([^ \t\n\r:"']{1,64}):([^ \t\n\r"'\x0b\x0c]{4,})"#,
    )
});
static NETRC: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?m)^[ \t]*machine[ \t]+[^ \t\n\r]+[ \t]+(?:login[ \t]+[^ \t\n\r]+[ \t]+)?password[ \t]+([^ \t\n\r]{6,})",
    )
});
/// Card numbers: 13-19 digits, single spaces or dashes allowed, not part of a
/// longer digit run.
static CARD: LazyLock<Regex> = LazyLock::new(|| re(r"[0-9](?:[ -]?[0-9]){12,18}"));

fn word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn alnum(c: char) -> bool {
    c.is_ascii_alphanumeric()
}

fn char_before(text: &str, index: usize) -> Option<char> {
    text.get(..index)?.chars().next_back()
}

fn char_at(text: &str, index: usize) -> Option<char> {
    text.get(index..)?.chars().next()
}

/// The Python pattern prefix `(?<![A-Za-z0-9_])`.
fn starts_at_word_boundary(text: &str, start: usize) -> bool {
    !char_before(text, start).is_some_and(word_char)
}

/// The Python pattern suffix `(?![A-Za-z0-9])`.
fn ends_before_alnum(text: &str, end: usize) -> bool {
    !char_at(text, end).is_some_and(alnum)
}

fn bounded_match(text: &str, start: usize, end: usize) -> bool {
    starts_at_word_boundary(text, start) && ends_before_alnum(text, end)
}

fn full_match(regex: &Regex, value: &str) -> bool {
    regex
        .find(value)
        .is_some_and(|m| m.start() == 0 && m.end() == value.len())
}

fn has_vendor_token(text: &str) -> bool {
    VENDOR.iter().any(|regex| {
        regex
            .find_iter(text)
            .any(|m| bounded_match(text, m.start(), m.end()))
    }) || has_aws_access_key(text)
}

/// AWS access key ids are a type prefix plus 16 uppercase alphanumerics that
/// must include a digit.
fn has_aws_access_key(text: &str) -> bool {
    AWS_KEY.find_iter(text).any(|m| {
        bounded_match(text, m.start(), m.end())
            && m.as_str()[4..].chars().any(|c| c.is_ascii_digit())
    })
}

fn has_sk_key(text: &str) -> bool {
    for m in SK_CANDIDATE.find_iter(text) {
        if !bounded_match(text, m.start(), m.end()) {
            continue;
        }
        let mut body = &m.as_str()[3..];
        if body.len() > SK_MAX_TOKEN {
            continue; // a run of text, not a key
        }
        // Skip up to three short namespace segments: proj-, ant-api03-.
        for _ in 0..3 {
            let Some((head, rest)) = body.split_once('-') else {
                break;
            };
            if !(1..=12).contains(&head.len())
                || !head.chars().all(|c| c.is_ascii_alphanumeric())
                || rest.len() < 32
            {
                break;
            }
            body = rest;
        }
        if body.chars().any(|c| c.is_ascii_digit())
            && body.chars().any(|c| c.is_ascii_alphabetic())
            && SK_RUN.is_match(body)
        {
            return true;
        }
    }
    false
}

fn has_jwt(text: &str) -> bool {
    JWT.find_iter(text)
        .any(|m| bounded_match(text, m.start(), m.end()) && is_jose_header(m.as_str()))
}

fn is_jose_header(segment: &str) -> bool {
    let mut padded = String::with_capacity(segment.len() + 4);
    padded.push_str(segment);
    while !padded.len().is_multiple_of(4) {
        padded.push('=');
    }
    match URL_SAFE.decode(padded.as_bytes()) {
        Ok(raw) => {
            let start = raw
                .iter()
                .position(|b| !b.is_ascii_whitespace())
                .unwrap_or(raw.len());
            let trimmed = &raw[start..];
            trimmed.starts_with(b"{")
                && (trimmed.windows(5).any(|w| w == b"\"alg\"")
                    || trimmed.windows(5).any(|w| w == b"\"enc\""))
        }
        Err(_) => false,
    }
}

/// Return true when `value` reads as a literal secret.
///
/// Rejects short values, shell or template expansions, paths, URLs, placeholder
/// words and single-character-class values such as a bare word, a bare number
/// or an ALL_CAPS variable name.
fn plausible_value(value: &str) -> bool {
    let value = value.trim_end_matches(['.', ')', ']', '}', '>']);
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if value.chars().count() < 8 || VALUE_BAD_LEAD.contains(first) {
        return false;
    }
    if value.contains("://") || value.contains("{{") || value.contains("${") {
        return false;
    }
    if value.chars().any(|c| CODE_CHARS.contains(&c))
        || full_match(&DOTTED_NAME, value)
        || full_match(&AT_NAME, value)
    {
        return false;
    }
    let lowered = value.to_lowercase();
    if PLACEHOLDER_WORDS.iter().any(|word| lowered.contains(word)) {
        return false;
    }
    let core: String = value.chars().filter(|c| *c != '_' && *c != '-').collect();
    let (mut has_lower, mut has_upper, mut has_digit, mut has_other) = (false, false, false, false);
    for ch in core.chars() {
        if ch.is_ascii_lowercase() {
            has_lower = true;
        } else if ch.is_ascii_uppercase() {
            has_upper = true;
        } else if ch.is_ascii_digit() {
            has_digit = true;
        } else {
            has_other = true;
        }
    }
    [has_lower, has_upper, has_digit, has_other]
        .iter()
        .filter(|set| **set)
        .count()
        >= 2
}

/// Match `KEY=VALUE` or `key: value` with a secret-bearing key.
///
/// The "=" form is accepted anywhere except as a URL query parameter, where the
/// clip is a URL the user wants to keep. The ":" form needs a quoted key (JSON)
/// or a key that starts its line (YAML, headers), so prose such as
/// "the secret: consistency" is left alone.
fn has_labelled_secret(text: &str) -> bool {
    let candidates = LABEL
        .captures_iter(text)
        .map(|caps| {
            let whole = caps.get(0).expect("group 0");
            (
                whole.start(),
                caps.get(1).expect("separator").as_str(),
                caps.get(2).expect("value"),
            )
        })
        .chain(LABEL_PASS_SUFFIX.captures_iter(text).map(|caps| {
            let keyword = caps.get(1).expect("pass");
            (
                keyword.start(),
                caps.get(2).expect("separator").as_str(),
                caps.get(3).expect("value"),
            )
        }));

    for (key_start, separator, value) in candidates {
        let floor = key_start.saturating_sub(64);
        let mut start = key_start;
        while start > floor && char_before(text, start).is_some_and(|c| IDENT_CHARS.contains(c)) {
            start -= char_before(text, start).expect("checked").len_utf8();
        }
        let before = char_before(text, start).unwrap_or('\n');
        if separator == "=" {
            if before == '?' || before == '&' {
                continue;
            }
        } else {
            let quoted = before == '"' || before == '\'';
            if !quoted {
                let line_start = text[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
                let lead = &text[line_start..start];
                if !lead.chars().all(|c| LINE_LEAD_CHARS.contains(c)) {
                    continue;
                }
            }
        }
        if plausible_value(value.as_str()) {
            return true;
        }
    }
    false
}

fn has_auth_header(text: &str) -> bool {
    for regex in [&*AUTH_HEADER, &*BEARER] {
        for caps in regex.captures_iter(text) {
            let whole = caps.get(0).expect("group 0");
            if starts_at_word_boundary(text, whole.start())
                && plausible_value(caps.get(1).expect("value").as_str())
            {
                return true;
            }
        }
    }
    false
}

fn has_cli_password(text: &str) -> bool {
    for line in text.split('\n') {
        if line.chars().count() > CLI_MAX_LINE {
            continue;
        }
        for caps in CLI_DASH_P.captures_iter(line) {
            let command = caps.get(1).expect("command");
            if !char_at(line, command.end()).is_some_and(word_char)
                && plausible_value(caps.get(2).expect("value").as_str())
            {
                return true;
            }
        }
        for caps in SSHPASS_P.captures_iter(line) {
            let whole = caps.get(0).expect("group 0");
            if starts_at_word_boundary(line, whole.start())
                && plausible_value(caps.get(1).expect("value").as_str())
            {
                return true;
            }
        }
        for caps in CLI_USER_PASS.captures_iter(line) {
            let command = caps.get(1).expect("command");
            if char_at(line, command.end()).is_some_and(word_char) {
                continue;
            }
            let user = caps.get(2).expect("user").as_str();
            let secret = caps.get(3).expect("secret").as_str();
            // "$USER:$TOKEN" is a shell expansion, not a password.
            if let (Some(user_first), Some(secret_first)) =
                (user.chars().next(), secret.chars().next())
                && !VALUE_BAD_LEAD.contains(user_first)
                && !VALUE_BAD_LEAD.contains(secret_first)
            {
                return true;
            }
        }
    }
    for caps in NETRC.captures_iter(text) {
        if plausible_value(caps.get(1).expect("value").as_str()) {
            return true;
        }
    }
    false
}

fn has_totp_seed(text: &str) -> bool {
    if OTPAUTH.is_match(text) {
        return true;
    }
    TOTP_LABEL.captures_iter(text).any(|caps| {
        let whole = caps.get(0).expect("group 0");
        let value = caps.get(1).expect("value").as_str();
        bounded_match(text, whole.start(), whole.end())
            && value.chars().any(|c| ('2'..='7').contains(&c))
    })
}

fn luhn_ok(digits: &str) -> bool {
    let mut total = 0u32;
    for (index, ch) in digits.chars().rev().enumerate() {
        let mut value = ch as u32 - '0' as u32;
        if index & 1 == 1 {
            value *= 2;
            if value > 9 {
                value -= 9;
            }
        }
        total += value;
    }
    total.is_multiple_of(10)
}

/// Check issuer prefix and length for the major card networks.
///
/// Thirteen-digit Visa and Maestro ranges are excluded: both are rare today and
/// both widen the false-positive surface.
fn issuer_ok(digits: &str) -> bool {
    let n = digits.len();
    let num = |range: std::ops::Range<usize>| -> i64 {
        digits.get(range).and_then(|s| s.parse().ok()).unwrap_or(0)
    };
    let p2 = num(0..2);
    let p3 = num(0..3);
    let p4 = num(0..4);
    if digits.starts_with('4') {
        return n == 16 || n == 19;
    }
    if (51..=55).contains(&p2) || (2221..=2720).contains(&p4) {
        return n == 16;
    }
    if p2 == 34 || p2 == 37 {
        return n == 15;
    }
    if p4 == 6011 || p2 == 65 || (644..=649).contains(&p3) {
        return n == 16 || n == 19;
    }
    if (3528..=3589).contains(&p4) || p2 == 62 {
        return (16..=19).contains(&n);
    }
    if p2 == 36 || p2 == 38 || p2 == 39 || (300..=305).contains(&p3) {
        return n == 14;
    }
    false
}

fn has_card_number(text: &str) -> bool {
    for m in CARD.find_iter(text) {
        let raw = m.as_str();
        if raw.contains(' ') && raw.contains('-') {
            continue;
        }
        // Not part of a longer digit run: `(?<!\d[ -])` and `(?![ -]?\d)`.
        let before_start = char_before(text, m.start());
        if before_start.is_some_and(alnum) {
            continue;
        }
        if let (Some(first), Some(second)) = (
            char_before(text, m.start()),
            char_before(text, m.start().saturating_sub(1)),
        ) && second.is_ascii_digit()
            && (first == ' ' || first == '-')
        {
            continue;
        }
        let after = char_at(text, m.end());
        if after.is_some_and(|c| c.is_ascii_alphanumeric()) {
            continue;
        }
        if let (Some(first), Some(second)) = (after, char_at(text, m.end() + 1))
            && (first == ' ' || first == '-')
            && second.is_ascii_digit()
        {
            continue;
        }
        let digits: String = raw.chars().filter(|c| *c != ' ' && *c != '-').collect();
        if (13..=19).contains(&digits.len()) && luhn_ok(&digits) && issuer_ok(&digits) {
            return true;
        }
    }
    false
}

/// Return true when `text` contains a concrete secret shape.
pub fn is_sensitive(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    let scanned: String = text.chars().take(SCAN_LIMIT).collect();
    let text = scanned.trim();
    if text.chars().count() < 8 {
        return false;
    }
    if has_vendor_token(text) {
        return true;
    }
    if has_sk_key(text) || PRIVATE_KEY.is_match(text) {
        return true;
    }
    if CRED_URL
        .find_iter(text)
        .any(|m| bounded_match(text, m.start(), m.end()))
    {
        return true;
    }
    if BARE_CRED.is_match(text) {
        return true;
    }
    if has_jwt(text) || has_labelled_secret(text) {
        return true;
    }
    if has_auth_header(text) || has_cli_password(text) {
        return true;
    }
    if has_totp_seed(text) || has_card_number(text) {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The strings from the original's bug report that the old rule flagged and
    /// deleted. Lifted from `tests/test_sensitive.py`.
    const BUG_REPORT_BENIGN: &[&str] = &[
        "https://github.com/MohammedEl-sayedAhmed/clipman",
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
        "https://docs.google.com/document/d/1a2b3c",
        "https://example.com:8080/api",
        "https://en.wikipedia.org/wiki/Clipboard_(computing)",
        "report_2024_final.pdf",
        "IMG_20240115.jpg",
        "feature/add-dark-mode",
        "v1.2.1",
        "2024-01-15T10:30:00Z",
        "#FF5733",
        "clipman@clipman.com",
        "Ctrl+Shift+V",
        "MohammedEl-sayedAhmed",
        "src/main.py:42",
        "a3f5b9c2d1e4",
    ];

    /// Card numbers from the payment networks' public test ranges.
    const CARD_NUMBERS: &[&str] = &[
        "4111111111111111",
        "4111 1111 1111 1111",
        "4111-1111-1111-1111",
        "378282246310005",
        "5500005555555559",
    ];

    #[test]
    fn everyday_clips_are_not_sensitive() {
        for text in BUG_REPORT_BENIGN {
            assert!(!is_sensitive(text), "flagged: {text}");
        }
    }

    #[test]
    fn card_numbers_are_sensitive() {
        for text in CARD_NUMBERS {
            assert!(is_sensitive(text), "missed: {text}");
        }
    }

    #[test]
    fn luhn_invalid_digits_are_not_sensitive() {
        assert!(!is_sensitive("4111111111111112"));
        assert!(!is_sensitive("1234567890123456"));
    }

    #[test]
    fn secret_inside_a_config_blob() {
        let text =
            "HOST=db.internal\nPORT=5432\nPASS".to_string() + "WORD=Xk9mP2vL5nQ8wR\nDEBUG=false";
        assert!(is_sensitive(&text));
    }

    #[test]
    fn benign_config_blob() {
        let text = "HOST=db.internal\nPORT=5432\nDEBUG=false\nWAYLAND_DISPLAY=wayland-0";
        assert!(!is_sensitive(text));
    }

    #[test]
    fn vendor_tokens_and_headers() {
        // Built from parts so no secret literal lives in the repository.
        let github = format!("ghp_{}", "abcdefghijklmnopqrstuv");
        let aws = format!("{}{}", "AKIA", "IOSFODNN7EXAMPLE");
        assert!(is_sensitive(&github));
        assert!(is_sensitive("Authorization: Bearer Abcdefghijk123xyz"));
        assert!(is_sensitive("password = Xk9mP2vL5nQ8wR"));
        assert!(is_sensitive(&aws));
        assert!(!is_sensitive("pk_live_abcdefghijklmnopqrst"));
        assert!(!is_sensitive(
            "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQ test@host"
        ));
    }

    #[test]
    fn hostile_clips_stay_linear() {
        use std::time::Instant;
        let size = 64 * 1024;
        for unit in ["sk-", "sk-a", "mysql ", "curl ", "http ", "sshpass "] {
            let mut text = String::with_capacity(size + unit.len());
            while text.len() < size {
                text.push_str(unit);
            }
            let text: String = text.chars().take(size).collect();
            let start = Instant::now();
            is_sensitive(&text);
            assert!(
                start.elapsed().as_secs_f64() < 1.0,
                "{unit} took {:?}",
                start.elapsed()
            );
        }
    }

    #[test]
    fn scan_stops_at_the_limit() {
        let padding = "x".repeat(SCAN_LIMIT + 10);
        let token = format!("{}{}", "AKIA", "IOSFODNN7EXAMPLE");
        assert!(is_sensitive(&format!("{token}\n{padding}")));
        assert!(!is_sensitive(&format!("{padding}\n{token}")));
    }
}
