//! Name and mint classification for the `.cv` pilot namespace policy.
//!
//! Encodes the enforcement matrix from `pilot/namespace-policy.md`:
//! which (NameClass, MintClass) combinations are eligible for registration.

use std::str::FromStr;
use std::sync::OnceLock;

use regex::Regex;

// Permissive by design: accepts chars that bech32 forbids (b, i, o, x, 1)
// to match the registry's own validation. Bech32 checksum validation happens
// later in nostr-sdk signature verification.
static NPUB_RE: OnceLock<Regex> = OnceLock::new();

fn npub_regex() -> &'static Regex {
    NPUB_RE.get_or_init(|| Regex::new(r"^npub1[a-z0-9]{58}$").expect("valid regex"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameClass {
    Npub,
    Custom,
    Testing,
}

impl NameClass {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            NameClass::Npub => "npub",
            NameClass::Custom => "custom",
            NameClass::Testing => "testing",
        }
    }
}

impl FromStr for NameClass {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "npub" => NameClass::Npub,
            "testing" => NameClass::Testing,
            _ => NameClass::Custom,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MintClass {
    Real,
    Test,
}

impl MintClass {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            MintClass::Real => "real",
            MintClass::Test => "test",
        }
    }
}

impl FromStr for MintClass {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "test" => MintClass::Test,
            _ => MintClass::Real,
        })
    }
}

#[must_use]
pub fn is_test_mint_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.contains("test") || lower.contains("fake")
}

#[must_use]
pub fn normalize_mint_url(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(u) => {
            let host = u.host_str().unwrap_or("").to_lowercase();
            format!("{}://{}", u.scheme(), host)
        }
        Err(_) => url.to_lowercase(),
    }
}

#[must_use]
pub fn classify_name(name_with_zone: &str) -> NameClass {
    let base = name_with_zone.strip_suffix(".cv").unwrap_or(name_with_zone);
    if npub_regex().is_match(base) {
        NameClass::Npub
    } else if base.starts_with("testing") {
        NameClass::Testing
    } else {
        NameClass::Custom
    }
}

#[must_use]
pub fn classify_payment(mint_url: &str, real_mints: &[String], test_mints: &[String]) -> MintClass {
    let normalized = normalize_mint_url(mint_url);
    if real_mints
        .iter()
        .any(|m| normalize_mint_url(m) == normalized)
    {
        return MintClass::Real;
    }
    if test_mints
        .iter()
        .any(|m| normalize_mint_url(m) == normalized)
    {
        return MintClass::Test;
    }
    if is_test_mint_url(&normalized) {
        MintClass::Test
    } else {
        MintClass::Real
    }
}

#[must_use]
pub fn allowed_combination(name: NameClass, mint: MintClass) -> bool {
    match (name, mint) {
        (NameClass::Npub, MintClass::Real) => true,
        (NameClass::Npub, MintClass::Test) => false,
        (NameClass::Custom, MintClass::Real) => true,
        (NameClass::Custom, MintClass::Test) => false,
        (NameClass::Testing, MintClass::Real) => true,
        (NameClass::Testing, MintClass::Test) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_npub() -> String {
        format!("npub1{}", "a".repeat(58))
    }

    #[test]
    fn allowed_all_six_combinations() {
        assert!(allowed_combination(NameClass::Npub, MintClass::Real));
        assert!(!allowed_combination(NameClass::Npub, MintClass::Test));
        assert!(allowed_combination(NameClass::Testing, MintClass::Real));
        assert!(allowed_combination(NameClass::Testing, MintClass::Test));
        assert!(allowed_combination(NameClass::Custom, MintClass::Real));
        assert!(!allowed_combination(NameClass::Custom, MintClass::Test));
    }

    #[test]
    fn test_mint_urls_detected() {
        assert!(is_test_mint_url("https://testnut.cashu.space"));
        assert!(is_test_mint_url("https://testnut.cashu.exchange"));
        assert!(is_test_mint_url("https://fake.cashu.me"));
        assert!(is_test_mint_url("https://TESTNUT.cashu.space"));
    }

    #[test]
    fn real_mint_urls_not_flagged() {
        assert!(!is_test_mint_url("https://mint.minibits.cash"));
        assert!(!is_test_mint_url("https://mint.nut.cash"));
    }

    #[test]
    fn normalize_lowercases_host() {
        assert_eq!(
            normalize_mint_url("https://Testnut.Cashu.Space"),
            "https://testnut.cashu.space"
        );
        assert_eq!(
            normalize_mint_url("https://MINT.Minibits.Cash:443/path"),
            "https://mint.minibits.cash"
        );
    }

    #[test]
    fn normalize_strips_path_and_port() {
        assert_eq!(
            normalize_mint_url("https://mint.example.com/v1"),
            "https://mint.example.com"
        );
        assert_eq!(
            normalize_mint_url("https://mint.example.com:700"),
            "https://mint.example.com"
        );
    }

    #[test]
    fn normalize_fallback_on_parse_error() {
        assert_eq!(normalize_mint_url("not a url at all"), "not a url at all");
        assert_eq!(normalize_mint_url("SCREAMING TEXT"), "screaming text");
    }

    #[test]
    fn override_real_mint_precedes_heuristic() {
        let real = vec!["https://testnut.reallyreal.com".to_string()];
        assert_eq!(
            classify_payment("https://testnut.reallyreal.com", &real, &[]),
            MintClass::Real
        );
    }

    #[test]
    fn override_test_mint_precedes_heuristic() {
        let test = vec!["https://mint.superlegit.cash".to_string()];
        assert_eq!(
            classify_payment("https://mint.superlegit.cash", &[], &test),
            MintClass::Test
        );
    }

    #[test]
    fn override_real_precedence_over_test() {
        let real = vec!["https://testnut.cashu.space".to_string()];
        let test = vec!["https://testnut.cashu.space".to_string()];
        assert_eq!(
            classify_payment("https://testnut.cashu.space", &real, &test),
            MintClass::Real
        );
    }

    #[test]
    fn classify_payment_falls_back_to_heuristic() {
        assert_eq!(
            classify_payment("https://testnut.cashu.space", &[], &[]),
            MintClass::Test
        );
        assert_eq!(
            classify_payment("https://mint.minibits.cash", &[], &[]),
            MintClass::Real
        );
    }

    #[test]
    fn classify_name_npub() {
        assert_eq!(
            classify_name(&format!("{}.cv", full_npub())),
            NameClass::Npub
        );
    }

    #[test]
    fn classify_name_npub_without_zone() {
        assert_eq!(classify_name(&full_npub()), NameClass::Npub);
    }

    #[test]
    fn classify_name_testing_prefix() {
        assert_eq!(classify_name("testingfoo.cv"), NameClass::Testing);
        assert_eq!(classify_name("testing.cv"), NameClass::Testing);
        assert_eq!(
            classify_name(
                "testingnpub1aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.cv"
            ),
            NameClass::Testing
        );
    }

    #[test]
    fn classify_name_custom() {
        assert_eq!(classify_name("alice.cv"), NameClass::Custom);
        assert_eq!(classify_name("bob.cv"), NameClass::Custom);
    }

    #[test]
    fn classify_name_npub_too_short_is_custom() {
        assert_eq!(classify_name("npub1short.cv"), NameClass::Custom);
    }

    #[test]
    fn classify_name_npub_too_long_is_custom() {
        let too_long = format!("npub1{}", "a".repeat(59));
        assert_eq!(classify_name(&format!("{too_long}.cv")), NameClass::Custom);
    }

    #[test]
    fn classify_name_npub_uppercase_is_custom() {
        let upper = format!("npub1{}", "A".repeat(58));
        assert_eq!(classify_name(&format!("{upper}.cv")), NameClass::Custom);
    }

    #[test]
    fn name_class_as_str() {
        assert_eq!(NameClass::Npub.as_str(), "npub");
        assert_eq!(NameClass::Custom.as_str(), "custom");
        assert_eq!(NameClass::Testing.as_str(), "testing");
    }

    #[test]
    fn mint_class_as_str() {
        assert_eq!(MintClass::Real.as_str(), "real");
        assert_eq!(MintClass::Test.as_str(), "test");
    }

    #[test]
    fn name_class_from_str() {
        assert_eq!(NameClass::from_str("npub").unwrap(), NameClass::Npub);
        assert_eq!(NameClass::from_str("testing").unwrap(), NameClass::Testing);
        assert_eq!(NameClass::from_str("custom").unwrap(), NameClass::Custom);
        assert_eq!(NameClass::from_str("unknown").unwrap(), NameClass::Custom);
    }

    #[test]
    fn mint_class_from_str() {
        assert_eq!(MintClass::from_str("real").unwrap(), MintClass::Real);
        assert_eq!(MintClass::from_str("test").unwrap(), MintClass::Test);
        assert_eq!(MintClass::from_str("unknown").unwrap(), MintClass::Real);
    }
}
