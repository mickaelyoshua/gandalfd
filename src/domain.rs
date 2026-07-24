#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DomainName(String); // Storage

// When searching avoid unnecessary allocation
impl std::borrow::Borrow<str> for DomainName {
    fn borrow(&self) -> &str {
        &self.0
    }
}

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum DomainError {
    #[error("must be lowercase for zero-cost query")]
    NotLowercase,
    #[error("invalid domain length")]
    InvalidLength,
    #[error("invalid label length")]
    InvalidLabelLength,
    #[error("label cannot start or end with a hyphen")]
    InvalidHyphen,
    #[error("invalid character in label")]
    InvalidCharacter,
}

impl TryFrom<&str> for DomainName {
    type Error = DomainError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let lower = s.to_ascii_lowercase();
        validate_domain(&lower)?;
        Ok(Self(lower))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DomainRef<'a>(&'a str); // Query

impl<'a> DomainRef<'a> {
    pub fn parse(s: &'a str) -> Result<Self, DomainError> {
        if s.chars().any(|c| c.is_ascii_uppercase()) {
            return Err(DomainError::NotLowercase);
        }
        validate_domain(s)?;
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        self.0
    }
}

// Domain restrictions follow RFC 1035 / RFC 8552 to guarantee that only
// standard compliant DNS names are inserted in memory, maintaining fast queries.
fn validate_domain(s: &str) -> Result<(), DomainError> {
    if s.is_empty() || s.len() > 253 {
        return Err(DomainError::InvalidLength);
    }
    for part in s.split('.') {
        if part.is_empty() || part.len() > 63 {
            return Err(DomainError::InvalidLabelLength);
        }
        if part.starts_with('-') || part.ends_with('-') {
            return Err(DomainError::InvalidHyphen);
        }
        if !part
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(DomainError::InvalidCharacter);
        }
    }
    Ok(())
}

pub trait Blocklist {
    fn is_blocked(&self, domain: DomainRef<'_>) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_validation() {
        assert!(validate_domain("google.com").is_ok());
        assert!(validate_domain("").is_err());
        assert!(validate_domain("invalid_char!").is_err());
        // RFC restrictions
        assert!(validate_domain("-invalid.com").is_err());
        assert!(validate_domain("invalid-.com").is_err());
        // Underscores are valid in DNS (RFC 8552) for SRV/TXT records
        assert!(validate_domain("_sip._tcp.example.com").is_ok());
    }

    #[test]
    fn domain_ref_parse() {
        assert!(DomainRef::parse("Google.com").is_err());
        assert_eq!(DomainRef::parse("google.com"), Ok(DomainRef("google.com")));
    }

    #[test]
    fn domain_name_try_from() {
        assert_eq!(
            DomainName::try_from("Google.Com"),
            Ok(DomainName("google.com".to_string()))
        );
        assert!(DomainName::try_from("invalid!").is_err());
    }
}
