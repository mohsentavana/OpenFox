use crate::ExtError;

/// What an extension is allowed to do. Permissions are declared in the
/// manifest and checked before host functions are linked: the default
/// extension can see nothing but its own log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// Read shield counters.
    Shield,
    /// Write log lines to the devtools console.
    Console,
    /// Observe navigation events.
    Navigation,
}

/// The manifest is a fixed-format text block, parsed with a purpose-built
/// reader rather than a config-framework dependency:
///
/// ```text
/// name = "shield-report"
/// version = "1.0"
/// permissions = shield, console, navigation
/// ```
#[derive(Debug, Clone)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub permissions: Vec<Permission>,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Manifest, ExtError> {
        let mut name = String::new();
        let mut version = String::new();
        let mut permissions = Vec::new();

        for line in text.lines() {
            let line = line.trim();
            let Some((key, value)) = line.split_once('=') else { continue };
            let key = key.trim();
            let value = value.trim().trim_matches('"');
            match key {
                "name" => name = value.to_owned(),
                "version" => version = value.to_owned(),
                "permissions" => {
                    for word in value.split(',') {
                        match word.trim() {
                            "shield" => permissions.push(Permission::Shield),
                            "console" => permissions.push(Permission::Console),
                            "navigation" => permissions.push(Permission::Navigation),
                            "" => {}
                            unknown => {
                                return Err(ExtError::Manifest(format!(
                                    "unknown permission: {unknown}"
                                )))
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        if name.is_empty() {
            return Err(ExtError::Manifest("missing name".into()));
        }
        if version.is_empty() {
            return Err(ExtError::Manifest("missing version".into()));
        }
        Ok(Manifest { name, version, permissions })
    }

    pub fn allows(&self, permission: Permission) -> bool {
        self.permissions.contains(&permission)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_validates() {
        let manifest =
            Manifest::parse("name = \"shield-report\"\nversion = \"1.0\"\npermissions = shield, console\n").unwrap();
        assert_eq!(manifest.name, "shield-report");
        assert!(manifest.allows(Permission::Shield));
        assert!(!manifest.allows(Permission::Navigation));
        assert!(Manifest::parse("version = \"1.0\"").is_err());
        assert!(Manifest::parse("name = \"x\"\nversion = \"1\"\npermissions = net").is_err());
    }
}
