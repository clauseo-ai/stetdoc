//! A small JSON value, writer and reader, for the `json` format.
//!
//! The neutral model has no `serde` derive, so the adapter writes and
//! reads JSON by hand. The shape is the neutral model's, which is what serde's default representation
//! would give: a struct is an object with its fields in declaration order, an enum without data is a
//! string, and an enum with data is an object with one key, the variant's name. The reader rejects what
//! `deny_unknown_fields` would: a field that is missing, repeated or not known. Numbers are whole
//! numbers in the range of `i64`, which is all the model holds.

use std::sync::Arc;

/// A JSON value. An object keeps its fields in the order they were written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Json {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A whole number.
    Int(i64),
    /// A string.
    Str(String),
    /// An array.
    Arr(Vec<Json>),
    /// An object.
    Obj(Vec<(String, Json)>),
}

/// Why a JSON text or a value was refused, and where in the text when it is known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct JsonError {
    /// What is wrong, in a sentence.
    pub message: String,
    /// The byte offset in the text, or `None` for a value that does not have the model's shape.
    pub at: Option<usize>,
}

impl JsonError {
    pub(crate) fn shape(message: impl Into<String>) -> JsonError {
        JsonError {
            message: message.into(),
            at: None,
        }
    }

    fn syntax(message: impl Into<String>, at: usize) -> JsonError {
        JsonError {
            message: message.into(),
            at: Some(at),
        }
    }
}

impl Json {
    /// Writes the value as compact JSON, with no whitespace.
    pub(crate) fn write(&self, out: &mut String) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(true) => out.push_str("true"),
            Json::Bool(false) => out.push_str("false"),
            Json::Int(n) => out.push_str(&n.to_string()),
            Json::Str(s) => write_string(s, out),
            Json::Arr(items) => {
                out.push('[');
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    item.write(out);
                }
                out.push(']');
            }
            Json::Obj(fields) => {
                out.push('{');
                for (index, (name, value)) in fields.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    write_string(name, out);
                    out.push(':');
                    value.write(out);
                }
                out.push('}');
            }
        }
    }

    /// Reads a JSON text. Nesting deeper than `max_depth` is refused, so a hostile text cannot take
    /// the stack.
    pub(crate) fn parse(text: &str, max_depth: usize) -> Result<Json, JsonError> {
        let mut reader = Reader {
            text,
            at: 0,
            max_depth,
        };
        reader.skip_space();
        let value = reader.value(0)?;
        reader.skip_space();
        if reader.at < text.len() {
            return Err(JsonError::syntax(
                "text after the end of the value",
                reader.at,
            ));
        }
        Ok(value)
    }
}

fn write_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

struct Reader<'a> {
    text: &'a str,
    at: usize,
    max_depth: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.at).copied()
    }

    fn skip_space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn expect(&mut self, byte: u8, what: &str) -> Result<(), JsonError> {
        if self.peek() == Some(byte) {
            self.at += 1;
            Ok(())
        } else {
            Err(JsonError::syntax(format!("expected {what}"), self.at))
        }
    }

    fn literal(&mut self, word: &str, value: Json) -> Result<Json, JsonError> {
        let end = self.at + word.len();
        if self.text.get(self.at..end) == Some(word) {
            self.at = end;
            Ok(value)
        } else {
            Err(JsonError::syntax("not a JSON value", self.at))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, JsonError> {
        if depth > self.max_depth {
            return Err(JsonError::syntax("nested too deeply", self.at));
        }
        match self.peek() {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => self.string().map(Json::Str),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'n') => self.literal("null", Json::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(JsonError::syntax("not a JSON value", self.at)),
        }
    }

    fn number(&mut self) -> Result<Json, JsonError> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.at += 1;
        }
        if matches!(self.peek(), Some(b'.' | b'e' | b'E')) {
            return Err(JsonError::syntax("only whole numbers are used", self.at));
        }
        self.text
            .get(start..self.at)
            .and_then(|digits| digits.parse::<i64>().ok())
            .map(Json::Int)
            .ok_or_else(|| JsonError::syntax("not a whole number in range", start))
    }

    fn array(&mut self, depth: usize) -> Result<Json, JsonError> {
        self.expect(b'[', "[")?;
        let mut items = Vec::new();
        self.skip_space();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(Json::Arr(items));
        }
        loop {
            self.skip_space();
            items.push(self.value(depth + 1)?);
            self.skip_space();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Ok(Json::Arr(items));
                }
                _ => return Err(JsonError::syntax("expected , or ]", self.at)),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Json, JsonError> {
        self.expect(b'{', "{")?;
        let mut fields = Vec::new();
        self.skip_space();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(Json::Obj(fields));
        }
        loop {
            self.skip_space();
            let name = self.string()?;
            self.skip_space();
            self.expect(b':', ":")?;
            self.skip_space();
            fields.push((name, self.value(depth + 1)?));
            self.skip_space();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(Json::Obj(fields));
                }
                _ => return Err(JsonError::syntax("expected , or }", self.at)),
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, JsonError> {
        let end = self.at + 4;
        let digits = self
            .text
            .get(self.at..end)
            .filter(|d| d.bytes().all(|b| b.is_ascii_hexdigit()))
            .and_then(|d| u32::from_str_radix(d, 16).ok())
            .ok_or_else(|| JsonError::syntax("expected four hexadecimal digits", self.at))?;
        self.at = end;
        Ok(digits)
    }

    fn string(&mut self) -> Result<String, JsonError> {
        self.expect(b'"', "a string")?;
        let mut out = String::new();
        loop {
            let run_start = self.at;
            while !matches!(self.peek(), None | Some(b'"' | b'\\' | 0..=0x1f)) {
                self.at += 1;
            }
            if let Some(run) = self.text.get(run_start..self.at) {
                out.push_str(run);
            }
            match self.peek() {
                Some(b'"') => {
                    self.at += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.at += 1;
                    let c = self.escape()?;
                    out.push(c);
                }
                Some(_) => {
                    return Err(JsonError::syntax(
                        "a control character in a string",
                        self.at,
                    ));
                }
                None => return Err(JsonError::syntax("a string that does not end", self.at)),
            }
        }
    }

    fn escape(&mut self) -> Result<char, JsonError> {
        let at = self.at;
        let byte = self
            .peek()
            .ok_or_else(|| JsonError::syntax("an escape that does not end", at))?;
        self.at += 1;
        let plain = match byte {
            b'"' => '"',
            b'\\' => '\\',
            b'/' => '/',
            b'b' => '\u{8}',
            b'f' => '\u{c}',
            b'n' => '\n',
            b'r' => '\r',
            b't' => '\t',
            b'u' => return self.unicode(at),
            _ => return Err(JsonError::syntax("an unknown escape", at)),
        };
        Ok(plain)
    }

    fn unicode(&mut self, at: usize) -> Result<char, JsonError> {
        let first = self.hex4()?;
        let code = if (0xd800..0xdc00).contains(&first) {
            if self.text.get(self.at..self.at + 2) != Some("\\u") {
                return Err(JsonError::syntax("a lone surrogate", at));
            }
            self.at += 2;
            let second = self.hex4()?;
            if !(0xdc00..0xe000).contains(&second) {
                return Err(JsonError::syntax("a lone surrogate", at));
            }
            0x10000 + ((first - 0xd800) << 10) + (second - 0xdc00)
        } else {
            first
        };
        char::from_u32(code).ok_or_else(|| JsonError::syntax("a lone surrogate", at))
    }
}

/// A type that has a JSON form.
pub(crate) trait ToJson {
    /// The value.
    fn to_json(&self) -> Json;
}

/// A type that is read from its JSON form.
pub(crate) trait FromJson: Sized {
    /// Reads the value, or says why it has the wrong shape.
    fn from_json(value: &Json) -> Result<Self, JsonError>;

    /// What a missing field stands for, if it may be missing.
    fn absent() -> Option<Self> {
        None
    }
}

/// The fields of an object, taken one by one, so that `done` can refuse one that no one took.
pub(crate) struct Fields<'a> {
    items: &'a [(String, Json)],
    taken: usize,
}

impl<'a> Fields<'a> {
    /// The fields of an object.
    pub(crate) fn of(value: &'a Json) -> Result<Fields<'a>, JsonError> {
        match value {
            Json::Obj(items) => Ok(Fields { items, taken: 0 }),
            _ => Err(JsonError::shape("expected an object")),
        }
    }

    /// Takes the field of this name.
    pub(crate) fn take<T: FromJson>(&mut self, name: &str) -> Result<T, JsonError> {
        match self.items.iter().find(|(key, _)| key == name) {
            Some((_, value)) => {
                self.taken += 1;
                T::from_json(value).map_err(|e| JsonError {
                    message: format!("{name}: {}", e.message),
                    at: e.at,
                })
            }
            None => {
                T::absent().ok_or_else(|| JsonError::shape(format!("the field {name} is missing")))
            }
        }
    }

    /// Refuses a field that was not taken: one that is not known, or one that is repeated.
    pub(crate) fn done(self) -> Result<(), JsonError> {
        if self.taken == self.items.len() {
            Ok(())
        } else {
            Err(JsonError::shape("a field is not known or is repeated"))
        }
    }
}

/// An enum with data: an object with one key, the variant's name, and the variant's payload.
pub(crate) fn tagged(name: &str, payload: Json) -> Json {
    Json::Obj(vec![(name.to_string(), payload)])
}

/// An object from `(name, value)` pairs.
pub(crate) fn object(fields: Vec<(&str, Json)>) -> Json {
    Json::Obj(
        fields
            .into_iter()
            .map(|(n, v)| (n.to_string(), v))
            .collect(),
    )
}

/// Splits an enum value into its variant name and payload: a string is a variant with no data, and an
/// object with one key is a variant with data.
pub(crate) fn variant(value: &Json) -> Result<(&str, Option<&Json>), JsonError> {
    match value {
        Json::Str(name) => Ok((name, None)),
        Json::Obj(items) => match items.as_slice() {
            [(name, payload)] => Ok((name, Some(payload))),
            _ => Err(JsonError::shape("expected an object with one key")),
        },
        _ => Err(JsonError::shape("expected a variant")),
    }
}

pub(crate) fn unknown_variant(of: &str, name: &str) -> JsonError {
    JsonError::shape(format!("{name:?} is not a {of}"))
}

impl ToJson for bool {
    fn to_json(&self) -> Json {
        Json::Bool(*self)
    }
}

impl FromJson for bool {
    fn from_json(value: &Json) -> Result<bool, JsonError> {
        match value {
            Json::Bool(b) => Ok(*b),
            _ => Err(JsonError::shape("expected a boolean")),
        }
    }
}

impl ToJson for u32 {
    fn to_json(&self) -> Json {
        Json::Int(i64::from(*self))
    }
}

impl FromJson for u32 {
    fn from_json(value: &Json) -> Result<u32, JsonError> {
        match value {
            Json::Int(n) => {
                u32::try_from(*n).map_err(|_| JsonError::shape("a number out of range"))
            }
            _ => Err(JsonError::shape("expected a number")),
        }
    }
}

impl ToJson for i64 {
    fn to_json(&self) -> Json {
        Json::Int(*self)
    }
}

impl FromJson for i64 {
    fn from_json(value: &Json) -> Result<i64, JsonError> {
        match value {
            Json::Int(n) => Ok(*n),
            _ => Err(JsonError::shape("expected a number")),
        }
    }
}

impl ToJson for String {
    fn to_json(&self) -> Json {
        Json::Str(self.clone())
    }
}

impl FromJson for String {
    fn from_json(value: &Json) -> Result<String, JsonError> {
        match value {
            Json::Str(s) => Ok(s.clone()),
            _ => Err(JsonError::shape("expected a string")),
        }
    }
}

impl ToJson for Box<str> {
    fn to_json(&self) -> Json {
        Json::Str(self.to_string())
    }
}

impl FromJson for Box<str> {
    fn from_json(value: &Json) -> Result<Box<str>, JsonError> {
        String::from_json(value).map(String::into_boxed_str)
    }
}

impl ToJson for Arc<str> {
    fn to_json(&self) -> Json {
        Json::Str(self.to_string())
    }
}

impl FromJson for Arc<str> {
    fn from_json(value: &Json) -> Result<Arc<str>, JsonError> {
        String::from_json(value).map(Arc::from)
    }
}

impl<T: ToJson> ToJson for Option<T> {
    fn to_json(&self) -> Json {
        self.as_ref().map_or(Json::Null, ToJson::to_json)
    }
}

impl<T: FromJson> FromJson for Option<T> {
    fn from_json(value: &Json) -> Result<Option<T>, JsonError> {
        match value {
            Json::Null => Ok(None),
            other => T::from_json(other).map(Some),
        }
    }

    fn absent() -> Option<Option<T>> {
        Some(None)
    }
}

impl<T: ToJson> ToJson for Vec<T> {
    fn to_json(&self) -> Json {
        Json::Arr(self.iter().map(ToJson::to_json).collect())
    }
}

impl<T: FromJson> FromJson for Vec<T> {
    fn from_json(value: &Json) -> Result<Vec<T>, JsonError> {
        match value {
            Json::Arr(items) => items.iter().map(T::from_json).collect(),
            _ => Err(JsonError::shape("expected an array")),
        }
    }
}
