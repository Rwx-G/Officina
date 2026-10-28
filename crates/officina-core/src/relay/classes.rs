//! Action classes (FR-8, AD-33): which classes this build can run. Until
//! stories 2.6 and 2.8 land, only `free` runs; `after-check` and `human` are
//! refused with their own reason code (D-20), a deliberate intermediate
//! state, and marked unavailable wherever actions are offered (D-17).

use super::catalogue::ActionClass;
use super::journal::RefusalReason;

/// True when this build runs actions of `class`.
///
/// ```
/// use officina_core::relay::catalogue::ActionClass;
/// use officina_core::relay::classes::is_available;
/// assert!(is_available(ActionClass::Free));
/// assert!(!is_available(ActionClass::Auto));
/// ```
pub fn is_available(class: ActionClass) -> bool {
    match class {
        ActionClass::Free => true,
        ActionClass::AfterCheck | ActionClass::Human | ActionClass::Auto => false,
    }
}

/// The class step of `officina run` (AD-29 step 6): a `free` action runs,
/// any other class is refused with [`RefusalReason::ClassNotAvailable`]
/// until its story lands (D-20).
pub fn decide(class: ActionClass) -> Result<(), RefusalReason> {
    if is_available(class) {
        Ok(())
    } else {
        Err(RefusalReason::ClassNotAvailable)
    }
}

/// The class as the floor writes it, for messages.
pub fn class_name(class: ActionClass) -> String {
    serde_json::to_value(class)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_free_runs_in_this_build() {
        assert_eq!(decide(ActionClass::Free), Ok(()));
        for class in [
            ActionClass::AfterCheck,
            ActionClass::Human,
            ActionClass::Auto,
        ] {
            assert_eq!(decide(class), Err(RefusalReason::ClassNotAvailable));
            assert!(!is_available(class));
        }
        assert_eq!(class_name(ActionClass::AfterCheck), "after-check");
    }
}
