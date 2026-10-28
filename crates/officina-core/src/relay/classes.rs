//! Action classes (FR-8, AD-33): which classes this build can run. Until
//! stories 2.6 and 2.8 land, only `free` runs; `after-check` and `human` are
//! refused with their own reason code (D-20), a deliberate intermediate
//! state, and marked unavailable wherever actions are offered (D-17).

use super::catalogue::ActionClass;

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
