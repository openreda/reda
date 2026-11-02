//! `MAJOR.MINOR.PATCH` release versions and their `vX.Y.Z` tags.

use std::fmt;
use std::str::FromStr;

use crate::Bump;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    /// The version a `vX.Y.Z` release tag names; `None` for any other tag.
    pub fn from_tag(tag: &str) -> Option<Version> {
        tag.strip_prefix('v')?.parse().ok()
    }

    /// The release tag for this version.
    pub fn tag(&self) -> String {
        format!("v{self}")
    }

    pub fn bump(self, bump: Bump) -> Version {
        match bump {
            Bump::Patch => Version {
                patch: self.patch + 1,
                ..self
            },
            Bump::Minor => Version {
                minor: self.minor + 1,
                patch: 0,
                ..self
            },
            Bump::Major => Version {
                major: self.major + 1,
                minor: 0,
                patch: 0,
            },
        }
    }
}

impl FromStr for Version {
    type Err = String;

    /// Parses `1.2.3`. Pre-release and build suffixes are not release versions.
    fn from_str(text: &str) -> Result<Self, String> {
        let bad = || format!("'{text}' is not a MAJOR.MINOR.PATCH version");
        let mut parts = text.split('.').map(|part| {
            // Only plain digits: u64's parser would also take a leading '+'.
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return Err(bad());
            }
            part.parse::<u64>().map_err(|_| bad())
        });
        let (Some(major), Some(minor), Some(patch), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(bad());
        };
        Ok(Version {
            major: major?,
            minor: minor?,
            patch: patch?,
        })
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}
