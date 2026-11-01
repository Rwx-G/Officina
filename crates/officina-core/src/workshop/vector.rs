//! Container roles and vectors (AD-102, D-243). This story declares the
//! types; story 6.6 adds the builders, which alone own the fixed options
//! and labels, so no caller can assemble a container's arguments by hand.

use serde::{Deserialize, Serialize};

use super::names::ObjectName;

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What a container is for (AD-102, AD-117, D-608, D-609).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum ContainerRole {
        Worker,
        Gateway,
        Packer,
        Inventory,
        Battery,
        BatteryGateway,
        Decoy,
        AuthProbe,
        Scanner,
    }
}

/// The arguments of one `docker create`: its name, the options, the image
/// (by id, D-207) and the command. Its fields are private and it has no
/// public constructor here; story 6.6's builders make every vector.
///
/// ```compile_fail,E0451
/// use officina_core::workshop::vector::{ContainerRole, ContainerVector};
/// let vector = ContainerVector {
///     role: ContainerRole::Worker,
///     name: todo!(),
///     options: vec!["--privileged".to_owned()],
///     image: "debian".to_owned(),
///     command: Vec::new(),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContainerVector {
    role: ContainerRole,
    name: ObjectName,
    options: Vec<String>,
    image: String,
    command: Vec<String>,
}

impl ContainerVector {
    pub fn role(&self) -> ContainerRole {
        self.role
    }

    pub fn name(&self) -> &ObjectName {
        &self.name
    }

    pub fn options(&self) -> &[String] {
        &self.options
    }

    pub fn image(&self) -> &str {
        &self.image
    }

    pub fn command(&self) -> &[String] {
        &self.command
    }

    /// A vector for this crate's own tests, until story 6.6's builders
    /// replace it (D-243).
    #[cfg(test)]
    pub(crate) fn sample(role: ContainerRole) -> ContainerVector {
        ContainerVector {
            role,
            name: ObjectName::parse("officina-worker-0123456789abcdef0123456789abcdef")
                .expect("a valid name"),
            options: vec![
                "--user".to_owned(),
                "1000:1000".to_owned(),
                "--read-only".to_owned(),
            ],
            image: "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                .to_owned(),
            command: vec!["run".to_owned()],
        }
    }
}
