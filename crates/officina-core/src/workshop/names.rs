//! Names and labels of the engine's objects (AD-98, D-604, D-605): the floor
//! key, a run's object names from its id, the floor's login volume and
//! credential target, image tags, and the label set every object carries.

use std::collections::BTreeMap;

use serde::Serialize;

use super::vector::ContainerRole;
use crate::quota::{fnv1a, normalise_path};

/// The label every Officina object carries, and the value it has.
pub const MANAGED_LABEL: (&str, &str) = ("officina.managed", "1");
/// The floor's key on every object.
pub const FLOOR_LABEL: &str = "officina.floor";
/// The Officina version that made the object.
pub const VERSION_LABEL: &str = "officina.version";
/// A run's id on each of its objects.
pub const RUN_LABEL: &str = "officina.run";
/// A container's [`ContainerRole`].
pub const ROLE_LABEL: &str = "officina.role";
/// The work order a run's objects belong to.
pub const WORK_ORDER_LABEL: &str = "officina.work_order";
/// The role label of the login container, which Officina never creates
/// itself (D-605): the author runs the command it prints.
pub const LOGIN_ROLE: &str = "login";
/// The tag of every image Officina builds (AD-99).
pub const IMAGE_TAG: &str = "local";
/// The longest object name accepted.
pub const MAX_NAME_CHARS: usize = 128;

/// A name or id `docker` receives as an argument: lowercase letters,
/// digits, `.`, `_`, `-` and `:`, starting with a letter or a digit, so no
/// name can read as an option.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct ObjectName(String);

impl ObjectName {
    /// The name, when it follows the rule.
    ///
    /// ```
    /// use officina_core::workshop::names::ObjectName;
    /// assert!(ObjectName::parse("officina-worker-0a").is_some());
    /// assert!(ObjectName::parse("--privileged").is_none());
    /// ```
    pub fn parse(name: &str) -> Option<ObjectName> {
        let first_ok = name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
        let all_ok = name.chars().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-' | ':')
        });
        (first_ok && all_ok && name.len() <= MAX_NAME_CHARS).then(|| ObjectName(name.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A run's id: 32 lowercase hex characters, its card's (AD-98, D-215).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct RunId(String);

impl RunId {
    /// The id, when it is 32 lowercase hex characters.
    pub fn parse(id: &str) -> Option<RunId> {
        (id.len() == 32
            && id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
        .then(|| RunId(id.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The floor key: AD-8's FNV-1a of the normalised floor path, 16 hex.
///
/// ```
/// use officina_core::workshop::names::floor_key;
/// assert_eq!(floor_key("C:\\Work\\officina.toml"), floor_key("c:/work/officina.toml"));
/// ```
pub fn floor_key(floor_path: &str) -> String {
    format!("{:016x}", fnv1a(&normalise_path(floor_path)))
}

/// The objects one run owns, named from its id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunNames {
    pub worker: ObjectName,
    pub gateway: ObjectName,
    pub packer: ObjectName,
    pub internal: ObjectName,
    pub egress: ObjectName,
    pub volume: ObjectName,
}

/// The prefixes of a run's objects, in [`RunNames`]'s order.
pub const RUN_PREFIXES: &[&str] = &[
    "officina-worker-",
    "officina-gateway-",
    "officina-pack-",
    "officina-int-",
    "officina-out-",
    "officina-vol-",
];

fn named(prefix: &str, run: &RunId) -> ObjectName {
    ObjectName(format!("{prefix}{}", run.as_str()))
}

impl RunNames {
    pub fn of(run: &RunId) -> RunNames {
        RunNames {
            worker: named(RUN_PREFIXES[0], run),
            gateway: named(RUN_PREFIXES[1], run),
            packer: named(RUN_PREFIXES[2], run),
            internal: named(RUN_PREFIXES[3], run),
            egress: named(RUN_PREFIXES[4], run),
            volume: named(RUN_PREFIXES[5], run),
        }
    }

    /// Every name, in [`RUN_PREFIXES`]'s order.
    pub fn all(&self) -> [&ObjectName; 6] {
        [
            &self.worker,
            &self.gateway,
            &self.packer,
            &self.internal,
            &self.egress,
            &self.volume,
        ]
    }
}

/// The floor's login volume (D-604, D-605).
pub fn login_volume(floor_key: &str) -> ObjectName {
    ObjectName(format!("officina-login-{floor_key}"))
}

/// The floor's Credential Manager target for the token kinds (D-604).
pub fn credential_target(floor_key: &str) -> String {
    format!("officina:workshop:{floor_key}")
}

/// The `cmdkey` line doctor prints to store the floor's token (D-38, D-604):
/// `/pass` without a value, so `cmdkey` asks for it.
pub fn cmdkey_line(floor_key: &str) -> String {
    format!(
        "cmdkey /generic:{} /user:workshop /pass",
        credential_target(floor_key)
    )
}

/// An image Officina builds, tagged `:local` (AD-99).
pub fn image_tag(name: &str) -> String {
    format!("{name}:{IMAGE_TAG}")
}

/// A project image's tag: the floor key's first 8 hex digits and the
/// project's name (AD-99).
pub fn project_image(floor_key: &str, project: &str) -> String {
    let short = floor_key.get(..8).unwrap_or(floor_key);
    image_tag(&format!("officina-project-{short}-{project}"))
}

/// The labels every object of the floor carries.
pub fn base_labels(floor_key: &str, version: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        (MANAGED_LABEL.0.to_owned(), MANAGED_LABEL.1.to_owned()),
        (FLOOR_LABEL.to_owned(), floor_key.to_owned()),
        (VERSION_LABEL.to_owned(), version.to_owned()),
    ])
}

/// The labels of a run's object in `role`.
pub fn run_labels(
    floor_key: &str,
    version: &str,
    run: &RunId,
    role: ContainerRole,
    work_order: &str,
) -> BTreeMap<String, String> {
    let mut labels = base_labels(floor_key, version);
    labels.insert(RUN_LABEL.to_owned(), run.as_str().to_owned());
    labels.insert(ROLE_LABEL.to_owned(), role.code().to_owned());
    labels.insert(WORK_ORDER_LABEL.to_owned(), work_order.to_owned());
    labels
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUN: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn object_names_derive_from_the_run_id() {
        let run = RunId::parse(RUN).unwrap();
        let names = RunNames::of(&run);
        for (name, prefix) in names.all().iter().zip(RUN_PREFIXES) {
            assert_eq!(name.as_str(), format!("{prefix}{RUN}"));
            assert!(ObjectName::parse(name.as_str()).is_some());
        }
        for bad in [
            "",
            "0123",
            &RUN.to_uppercase(),
            &format!("{RUN}0"),
            "g123456789abcdef0123456789abcdef",
        ] {
            assert!(RunId::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn every_role_has_its_label() {
        let run = RunId::parse(RUN).unwrap();
        for role in ContainerRole::ALL {
            let labels = run_labels("00000000000000aa", "0.1.0", &run, *role, "wo-1");
            assert_eq!(labels[ROLE_LABEL], role.code());
            assert_eq!(labels[MANAGED_LABEL.0], MANAGED_LABEL.1);
            assert_eq!(labels[RUN_LABEL], RUN);
            assert_eq!(labels[FLOOR_LABEL], "00000000000000aa");
        }
        assert!(
            ContainerRole::ALL.iter().all(|r| r.code() != LOGIN_ROLE),
            "the login container is not one Officina creates"
        );
    }

    #[test]
    fn the_floor_key_is_the_quota_fnv() {
        let path = "C:/Path/To/officina.toml";
        assert_eq!(
            floor_key(path),
            format!("{:016x}", fnv1a(&normalise_path(path)))
        );
        assert_eq!(floor_key(path).len(), 16);
        assert_eq!(
            project_image(&floor_key(path), "api"),
            format!("officina-project-{}-api:local", &floor_key(path)[..8])
        );
    }

    #[test]
    fn the_login_volume_and_credential_target_carry_the_floor_key() {
        let key = floor_key("C:/work/officina.toml");
        assert_eq!(login_volume(&key).as_str(), format!("officina-login-{key}"));
        assert_eq!(credential_target(&key), format!("officina:workshop:{key}"));
        assert!(cmdkey_line(&key).ends_with("/user:workshop /pass"));
        assert!(cmdkey_line(&key).contains(&credential_target(&key)));
    }
}
