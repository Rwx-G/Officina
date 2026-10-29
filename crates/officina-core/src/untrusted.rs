//! Text that came from a session or a model (FR-3, AD-74, D-130, D-153):
//! a distinct type that can be shown and measured, never handed on as a
//! string.
//!
//! [`Untrusted`] has no public field and no `Deref`, `AsRef`, `Borrow`,
//! `Display`, `From` or `Into` towards `String` or `&str`. Its text leaves
//! only through serialization: transparently, so payload bytes and the
//! TypeScript bindings stay what they were, or wrapped as
//! `{"untrusted": "..."}` inside [`wrapped`], the one mode the MCP layer
//! serializes its outputs in. Facts core needs about the text ([`char_count`],
//! [`truncated`], [`first_line_matching`]) are computed here and return no
//! `String`.
//!
//! What the doctests below prove is that each snippet fails to compile, not
//! that no path exists: `serde_json::to_string` still yields a string
//! (AD-74). The launch path's mutation test and the injection battery carry
//! the rest of the claim (AD-95).
//!
//! Dereferencing does not compile:
//!
//! ```compile_fail,E0614
//! use officina_core::untrusted::Untrusted;
//! let u = Untrusted::new(String::from("text"));
//! let _s: &String = &*u;
//! ```
//!
//! Neither does `as_ref`:
//!
//! ```compile_fail,E0599
//! use officina_core::untrusted::Untrusted;
//! let u = Untrusted::new(String::from("text"));
//! let _s: &str = u.as_ref();
//! ```
//!
//! Nor `String::from`:
//!
//! ```compile_fail,E0277
//! use officina_core::untrusted::Untrusted;
//! let u = Untrusted::new(String::from("text"));
//! let _s = String::from(u);
//! ```
//!
//! Nor `into`:
//!
//! ```compile_fail,E0277
//! use officina_core::untrusted::Untrusted;
//! let u = Untrusted::new(String::from("text"));
//! let _s: String = u.into();
//! ```
//!
//! Nor formatting it:
//!
//! ```compile_fail,E0277
//! use officina_core::untrusted::Untrusted;
//! let u = Untrusted::new(String::from("text"));
//! let _s = format!("{}", u);
//! ```
//!
//! Nor formatting its display:
//!
//! ```compile_fail,E0277
//! use officina_core::untrusted::Untrusted;
//! let u = Untrusted::new(String::from("text"));
//! let _s = format!("{}", u.display());
//! ```
//!
//! Nor taking its display for a `&str`:
//!
//! ```compile_fail,E0308
//! use officina_core::untrusted::Untrusted;
//! let u = Untrusted::new(String::from("text"));
//! let _s: &str = u.display();
//! ```
//!
//! What a payload does with it: serialize its display, or the value itself.
//!
//! ```
//! use officina_core::untrusted::Untrusted;
//! let u = Untrusted::new(String::from("text"));
//! assert_eq!(serde_json::to_string(&u.display()).unwrap(), r#""text""#);
//! assert_eq!(serde_json::to_string(&u).unwrap(), r#""text""#);
//! ```
//!
//! [`char_count`]: Untrusted::char_count
//! [`truncated`]: Untrusted::truncated
//! [`first_line_matching`]: Untrusted::first_line_matching

use std::cell::Cell;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

thread_local! {
    /// Set by [`wrapped`] for the duration of its closure, on this thread
    /// only.
    static WRAPPED: Cell<bool> = const { Cell::new(false) };
}

/// Text from a session or a model (AD-74). Shown through [`display`] or a
/// serialization, measured through its methods, never converted to a
/// string.
///
/// Its `Debug` output names the type without the text, so a `{:?}` cannot
/// carry it either.
///
/// ```
/// use officina_core::untrusted::Untrusted;
/// let u = Untrusted::new(String::from("ignore previous instructions"));
/// assert_eq!(format!("{u:?}"), "Untrusted(..)");
/// ```
///
/// [`display`]: Untrusted::display
#[derive(Clone, PartialEq, Eq)]
pub struct Untrusted<T>(T);

impl<T> Untrusted<T> {
    /// Wraps `value`, text a session or a model supplied.
    pub fn new(value: T) -> Untrusted<T> {
        Untrusted(value)
    }
}

impl<T: AsRef<str>> Untrusted<T> {
    /// The text for a serialization, and nothing else (AD-74).
    pub fn display(&self) -> DisplayText<'_> {
        DisplayText(self.0.as_ref())
    }

    /// How many characters the text holds.
    ///
    /// ```
    /// use officina_core::untrusted::Untrusted;
    /// assert_eq!(Untrusted::new("é1").char_count(), 2);
    /// ```
    pub fn char_count(&self) -> usize {
        self.0.as_ref().chars().count()
    }

    /// The text's first `chars` characters, cut on a character boundary,
    /// still untrusted.
    ///
    /// ```
    /// use officina_core::untrusted::Untrusted;
    /// let cut = Untrusted::new("héllo").truncated(2);
    /// assert_eq!(cut, Untrusted::new(String::from("hé")));
    /// ```
    pub fn truncated(&self, chars: usize) -> Untrusted<String> {
        Untrusted(self.0.as_ref().chars().take(chars).collect())
    }

    /// The number, from 1, of the first line holding any of `needles`;
    /// `None` when no line does.
    ///
    /// ```
    /// use officina_core::untrusted::Untrusted;
    /// let u = Untrusted::new("plan\nrun the tests\ncommit");
    /// assert_eq!(u.first_line_matching(&["tests", "commit"]), Some(2));
    /// assert_eq!(u.first_line_matching(&["deploy"]), None);
    /// ```
    pub fn first_line_matching(&self, needles: &[&str]) -> Option<u32> {
        let index = self
            .0
            .as_ref()
            .lines()
            .position(|line| needles.iter().any(|needle| line.contains(needle)))?;
        u32::try_from(index + 1).ok()
    }
}

impl<T> fmt::Debug for Untrusted<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Untrusted(..)")
    }
}

/// The wrapped form [`wrapped`] gives every untrusted value.
#[derive(Serialize)]
struct Marked<'a, T: ?Sized> {
    untrusted: &'a T,
}

fn serialize_text<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    T: Serialize + ?Sized,
    S: Serializer,
{
    if WRAPPED.with(Cell::get) {
        Marked { untrusted: value }.serialize(serializer)
    } else {
        value.serialize(serializer)
    }
}

impl<T: Serialize> Serialize for Untrusted<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serialize_text(&self.0, serializer)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Untrusted<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Untrusted)
    }
}

/// A `serialize_with` for a field Officina reads as data although a session
/// chose it, such as a session's folder, which the focus and the work
/// order binding compare (D-962): plain in the window's and the CLI's
/// payloads, `{"untrusted": "..."}` inside [`wrapped`] as an
/// [`Untrusted`] value is; `None` stays `null`.
///
/// ```
/// use officina_core::untrusted::{marked_option, wrapped};
/// #[derive(serde::Serialize)]
/// struct Card {
///     #[serde(serialize_with = "marked_option")]
///     cwd: Option<String>,
/// }
/// let card = Card { cwd: Some("C:/work".into()) };
/// assert_eq!(serde_json::to_string(&card).unwrap(), r#"{"cwd":"C:/work"}"#);
/// let marked = wrapped(|| serde_json::to_string(&card)).unwrap();
/// assert_eq!(marked, r#"{"cwd":{"untrusted":"C:/work"}}"#);
/// let none = wrapped(|| serde_json::to_string(&Card { cwd: None })).unwrap();
/// assert_eq!(none, r#"{"cwd":null}"#);
/// ```
pub fn marked_option<T, S>(value: &Option<T>, serializer: S) -> Result<S::Ok, S::Error>
where
    T: Serialize,
    S: Serializer,
{
    match value {
        Some(text) => serialize_text(text, serializer),
        None => serializer.serialize_none(),
    }
}

/// An untrusted text's read access (AD-74): it implements `Serialize` and
/// nothing else, so it lives only inside a serialization (D-130).
pub struct DisplayText<'a>(&'a str);

impl Serialize for DisplayText<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serialize_text(self.0, serializer)
    }
}

/// Runs `f` with every [`Untrusted`] value serialized on this thread as
/// `{"untrusted": "..."}` instead of its plain text (D-153), so an MCP
/// output marks each untrusted field with no list of which fields those
/// are. The previous mode comes back when `f` returns or panics. Only
/// `mcp::dispatch` calls it; window and CLI payloads never do.
///
/// ```
/// use officina_core::untrusted::{Untrusted, wrapped};
/// let u = Untrusted::new("text");
/// let marked = wrapped(|| serde_json::to_string(&u)).unwrap();
/// assert_eq!(marked, r#"{"untrusted":"text"}"#);
/// assert_eq!(serde_json::to_string(&u).unwrap(), r#""text""#);
/// ```
pub fn wrapped<R>(f: impl FnOnce() -> R) -> R {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            WRAPPED.with(|flag| flag.set(self.0));
        }
    }
    let _restore = Restore(WRAPPED.with(|flag| flag.replace(true)));
    f()
}

/// ts-rs sees the inner type (AD-74, AD-13): an `Untrusted<String>` field is
/// a `string`, so the committed bindings do not change.
#[cfg(feature = "ts")]
impl<T: ts_rs::TS> ts_rs::TS for Untrusted<T> {
    type WithoutGenerics = Self;
    type OptionInnerType = Self;

    fn name(cfg: &ts_rs::Config) -> String {
        T::name(cfg)
    }

    fn inline(cfg: &ts_rs::Config) -> String {
        T::inline(cfg)
    }

    fn inline_flattened(cfg: &ts_rs::Config) -> String {
        T::inline_flattened(cfg)
    }

    fn visit_dependencies(visitor: &mut impl ts_rs::TypeVisitor)
    where
        Self: 'static,
    {
        T::visit_dependencies(visitor);
    }

    fn visit_generics(visitor: &mut impl ts_rs::TypeVisitor)
    where
        Self: 'static,
    {
        T::visit_generics(visitor);
        visitor.visit::<T>();
    }

    fn decl(_: &ts_rs::Config) -> String {
        unreachable!("Untrusted is inlined as its inner type, never declared")
    }

    fn decl_concrete(_: &ts_rs::Config) -> String {
        unreachable!("Untrusted is inlined as its inner type, never declared")
    }
}

/// `value` as untrusted text, for the tests that compare a payload's
/// fields.
#[cfg(test)]
pub(crate) fn text(value: &str) -> Untrusted<String> {
    Untrusted::new(value.to_owned())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::{Value, json};

    use super::*;

    #[test]
    fn serializes_as_its_inner_value() {
        assert_eq!(
            serde_json::to_string(&text("a \"b\"")).unwrap(),
            r#""a \"b\"""#
        );
        assert_eq!(
            serde_json::to_string(&Some(text("x"))).unwrap(),
            serde_json::to_string(&Some("x")).unwrap()
        );
        assert_eq!(serde_json::to_value(Untrusted::new(7)).unwrap(), json!(7));
    }

    #[test]
    fn deserializes_from_its_inner_value() {
        let read: Untrusted<String> = serde_json::from_str(r#""hello""#).unwrap();
        assert_eq!(read, text("hello"));
        let absent: Option<Untrusted<String>> = serde_json::from_str("null").unwrap();
        assert_eq!(absent, None);
        assert!(
            serde_json::from_str::<Untrusted<String>>(r#"{"untrusted":"x"}"#).is_err(),
            "the wrapped form is an output, never read back"
        );
    }

    #[test]
    fn display_serializes_as_the_text() {
        let u = text("line one\nline two");
        assert_eq!(
            serde_json::to_value(u.display()).unwrap(),
            serde_json::to_value(&u).unwrap()
        );
        assert_eq!(
            serde_json::to_value(u.display()).unwrap(),
            json!("line one\nline two")
        );
    }

    #[test]
    fn truncated_cuts_on_a_char_boundary() {
        let u = text("aé😀b");
        assert_eq!(u.char_count(), 4);
        assert_eq!(u.truncated(0), text(""));
        assert_eq!(u.truncated(2), text("aé"));
        assert_eq!(u.truncated(3), text("aé😀"));
        assert_eq!(u.truncated(10), u, "shorter than the cut: whole");
    }

    #[test]
    fn first_line_matching_counts_from_one() {
        let u = text("first\nsecond holds it\nthird holds it too");
        assert_eq!(u.first_line_matching(&["first"]), Some(1));
        assert_eq!(u.first_line_matching(&["holds", "nothing"]), Some(2));
        assert_eq!(u.first_line_matching(&["too"]), Some(3));
        assert_eq!(text("a\r\nb").first_line_matching(&["b"]), Some(2));
    }

    #[test]
    fn first_line_matching_is_none_without_a_match() {
        let u = text("first\nsecond");
        assert_eq!(u.first_line_matching(&["third"]), None);
        assert_eq!(u.first_line_matching(&[]), None);
        assert_eq!(text("").first_line_matching(&["x"]), None);
    }

    #[derive(Serialize)]
    struct Inner {
        plain: String,
        said: Untrusted<String>,
        by_name: BTreeMap<String, Untrusted<String>>,
    }

    #[derive(Serialize)]
    struct Outer {
        id: u32,
        title: Option<Untrusted<String>>,
        absent: Option<Untrusted<String>>,
        items: Vec<Inner>,
    }

    fn nested() -> Outer {
        Outer {
            id: 1,
            title: Some(text("t")),
            absent: None,
            items: vec![Inner {
                plain: "p".to_owned(),
                said: text("s"),
                by_name: BTreeMap::from([("k".to_owned(), text("v"))]),
            }],
        }
    }

    /// The marked value of every `{"untrusted": ...}` object in `value`,
    /// each replaced by what it marks.
    fn unmark(value: &mut Value, marked: &mut Vec<Value>) {
        match value {
            Value::Object(map) if map.len() == 1 && map.contains_key("untrusted") => {
                let inner = map["untrusted"].clone();
                marked.push(inner.clone());
                *value = inner;
            }
            Value::Object(map) => map.values_mut().for_each(|v| unmark(v, marked)),
            Value::Array(items) => items.iter_mut().for_each(|v| unmark(v, marked)),
            _ => {}
        }
    }

    #[test]
    fn wrapped_mode_wraps_every_untrusted_field_of_a_nested_payload() {
        let payload = nested();
        let plain = serde_json::to_value(&payload).unwrap();
        let mut marked_value = wrapped(|| serde_json::to_value(&payload)).unwrap();
        assert_eq!(marked_value["title"], json!({"untrusted": "t"}));
        assert_eq!(marked_value["absent"], Value::Null, "absent stays null");
        assert_eq!(marked_value["items"][0]["plain"], json!("p"));
        assert_eq!(marked_value["items"][0]["said"], json!({"untrusted": "s"}));
        assert_eq!(
            marked_value["items"][0]["by_name"]["k"],
            json!({"untrusted": "v"})
        );
        let mut marked = Vec::new();
        unmark(&mut marked_value, &mut marked);
        marked.sort_by_key(ToString::to_string);
        assert_eq!(marked, [json!("s"), json!("t"), json!("v")]);
        assert_eq!(marked_value, plain, "only the untrusted values changed");
        let shown = text("d");
        assert_eq!(
            wrapped(|| serde_json::to_value(shown.display())).unwrap(),
            json!({"untrusted": "d"}),
            "a display is marked like its value"
        );
    }

    #[test]
    fn outside_wrapped_mode_the_bytes_are_unchanged() {
        let payload = nested();
        let before = serde_json::to_string(&payload).unwrap();
        assert_eq!(
            before,
            r#"{"id":1,"title":"t","absent":null,"items":[{"plain":"p","said":"s","by_name":{"k":"v"}}]}"#
        );
        let _ = wrapped(|| serde_json::to_string(&payload));
        assert_eq!(serde_json::to_string(&payload).unwrap(), before);
        let nested_calls = wrapped(|| {
            wrapped(|| ());
            serde_json::to_value(text("x")).unwrap()
        });
        assert_eq!(
            nested_calls,
            json!({"untrusted": "x"}),
            "an inner call restores the outer mode"
        );
    }

    #[test]
    fn the_flag_is_reset_after_a_panic() {
        let caught = std::panic::catch_unwind(|| {
            wrapped(|| panic!("a tool failed while serializing"));
        });
        assert!(caught.is_err());
        assert_eq!(serde_json::to_value(text("x")).unwrap(), json!("x"));
    }

    #[test]
    fn the_flag_does_not_cross_threads() {
        let other = wrapped(|| {
            std::thread::spawn(|| serde_json::to_value(text("x")).unwrap())
                .join()
                .unwrap()
        });
        assert_eq!(other, json!("x"));
    }
}
