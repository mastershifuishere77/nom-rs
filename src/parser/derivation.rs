use crate::types::{Derivation, OutputName, StorePath};
use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct ParsedDerivation {
    pub outputs: HashMap<OutputName, StorePath>,
    pub input_drvs: HashMap<Derivation, Vec<OutputName>>,
    pub input_srcs: Vec<StorePath>,
    pub platform: String,
    pub builder: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub pname: Option<String>,
}

#[derive(Debug)]
pub enum DerivationParseError {
    UnexpectedToken(String),
    UnexpectedEof,
    InvalidStorePath(String),
}

struct Lexer<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self { input, pos: 0 }
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() && self.input[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_whitespace();
        if self.pos < self.input.len() {
            Some(self.input[self.pos])
        } else {
            None
        }
    }

    fn next_char(&mut self) -> Option<u8> {
        self.skip_whitespace();
        if self.pos < self.input.len() {
            let b = self.input[self.pos];
            self.pos += 1;
            Some(b)
        } else {
            None
        }
    }

    fn expect(&mut self, expected: u8) -> Result<(), DerivationParseError> {
        let b = self
            .next_char()
            .ok_or(DerivationParseError::UnexpectedEof)?;
        if b == expected {
            Ok(())
        } else {
            Err(DerivationParseError::UnexpectedToken(format!(
                "expected '{}', got '{}'",
                expected as char, b as char
            )))
        }
    }

    fn parse_string(&mut self) -> Result<String, DerivationParseError> {
        self.expect(b'"')?;
        let start = self.pos;
        if let Some(rel) = memchr::memchr2(b'"', b'\\', &self.input[start..]) {
            let idx = start + rel;
            if self.input[idx] == b'"' {
                // Fast path: no escapes in the entire string (true for almost all store paths)
                self.pos = idx + 1;
                return std::str::from_utf8(&self.input[start..idx])
                    .map(|s| s.to_string())
                    .map_err(|e| DerivationParseError::UnexpectedToken(e.to_string()));
            }
        }

        // General path: string contains escape sequences
        let mut result = Vec::with_capacity(32);
        let mut escaped = false;
        while self.pos < self.input.len() {
            let b = self.input[self.pos];
            self.pos += 1;
            if escaped {
                match b {
                    b'n' => result.push(b'\n'),
                    b'r' => result.push(b'\r'),
                    b't' => result.push(b'\t'),
                    b'\\' => result.push(b'\\'),
                    b'"' => result.push(b'"'),
                    other => result.push(other),
                }
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                return String::from_utf8(result)
                    .map_err(|e| DerivationParseError::UnexpectedToken(e.to_string()));
            } else {
                result.push(b);
            }
        }
        Err(DerivationParseError::UnexpectedEof)
    }

    fn skip_string(&mut self) -> Result<(), DerivationParseError> {
        self.expect(b'"')?;
        while self.pos < self.input.len() {
            if let Some(rel) = memchr::memchr2(b'"', b'\\', &self.input[self.pos..]) {
                let idx = self.pos + rel;
                if self.input[idx] == b'"' {
                    self.pos = idx + 1;
                    return Ok(());
                } else {
                    // Backslash escape: skip the backslash and the escaped byte
                    self.pos = idx + 2;
                }
            } else {
                return Err(DerivationParseError::UnexpectedEof);
            }
        }
        Err(DerivationParseError::UnexpectedEof)
    }

    fn parse_ident(&mut self) -> Result<String, DerivationParseError> {
        self.skip_whitespace();
        let start = self.pos;
        while self.pos < self.input.len()
            && (self.input[self.pos].is_ascii_alphanumeric() || self.input[self.pos] == b'_')
        {
            self.pos += 1;
        }
        if start == self.pos {
            return Err(DerivationParseError::UnexpectedToken(
                "expected identifier".into(),
            ));
        }
        std::str::from_utf8(&self.input[start..self.pos])
            .map(|s| s.to_string())
            .map_err(|e| DerivationParseError::UnexpectedToken(e.to_string()))
    }
}

pub fn parse_derivation_content(content: &str) -> Result<ParsedDerivation, DerivationParseError> {
    let mut lexer = Lexer::new(content.as_bytes());
    let ident = lexer.parse_ident()?;
    if ident != "Derive" {
        return Err(DerivationParseError::UnexpectedToken(format!(
            "expected Derive, got {}",
            ident
        )));
    }
    lexer.expect(b'(')?;

    // 1. outputs: [ ("out", "/nix/store/...", "hashAlgo", "hash"), ... ]
    let mut outputs = HashMap::new();
    lexer.expect(b'[')?;
    if lexer.peek() != Some(b']') {
        loop {
            lexer.expect(b'(')?;
            let out_name = lexer.parse_string()?;
            lexer.expect(b',')?;
            let out_path_str = lexer.parse_string()?;
            lexer.expect(b',')?;
            lexer.skip_string()?;
            lexer.expect(b',')?;
            lexer.skip_string()?;
            lexer.expect(b')')?;

            if let Some(store_path) = StorePath::parse(&out_path_str) {
                outputs.insert(OutputName::parse(&out_name), store_path);
            }

            if lexer.peek() == Some(b',') {
                lexer.next_char();
            } else {
                break;
            }
        }
    }
    lexer.expect(b']')?;
    lexer.expect(b',')?;

    // 2. inputDrvs: [ ("/nix/store/...drv", ["out", ...]), ... ]
    let mut input_drvs = HashMap::new();
    lexer.expect(b'[')?;
    if lexer.peek() != Some(b']') {
        loop {
            lexer.expect(b'(')?;
            let drv_path_str = lexer.parse_string()?;
            lexer.expect(b',')?;
            lexer.expect(b'[')?;
            let mut drv_outputs = Vec::new();
            if lexer.peek() != Some(b']') {
                loop {
                    let out_name = lexer.parse_string()?;
                    drv_outputs.push(OutputName::parse(&out_name));
                    if lexer.peek() == Some(b',') {
                        lexer.next_char();
                    } else {
                        break;
                    }
                }
            }
            lexer.expect(b']')?;
            lexer.expect(b')')?;

            if let Some(drv) = Derivation::parse(&drv_path_str) {
                input_drvs.insert(drv, drv_outputs);
            }

            if lexer.peek() == Some(b',') {
                lexer.next_char();
            } else {
                break;
            }
        }
    }
    lexer.expect(b']')?;
    lexer.expect(b',')?;

    // 3. inputSrcs: [ "/nix/store/...", ... ]
    let mut input_srcs = Vec::new();
    lexer.expect(b'[')?;
    if lexer.peek() != Some(b']') {
        loop {
            let src_str = lexer.parse_string()?;
            if let Some(sp) = StorePath::parse(&src_str) {
                input_srcs.push(sp);
            }
            if lexer.peek() == Some(b',') {
                lexer.next_char();
            } else {
                break;
            }
        }
    }
    lexer.expect(b']')?;
    lexer.expect(b',')?;

    // 4. platform
    let platform = lexer.parse_string()?;
    lexer.expect(b',')?;

    // 5. builder (skip unused string)
    lexer.skip_string()?;
    lexer.expect(b',')?;

    // 6. args: [ "arg1", ... ] (skip unused strings)
    lexer.expect(b'[')?;
    if lexer.peek() != Some(b']') {
        loop {
            lexer.skip_string()?;
            if lexer.peek() == Some(b',') {
                lexer.next_char();
            } else {
                break;
            }
        }
    }
    lexer.expect(b']')?;
    lexer.expect(b',')?;

    // 7. env: [ ("key", "val"), ... ] (only extract pname, skip all other values)
    let mut pname = None;
    lexer.expect(b'[')?;
    if lexer.peek() != Some(b']') {
        loop {
            lexer.expect(b'(')?;
            let key = lexer.parse_string()?;
            lexer.expect(b',')?;
            if key == "pname" {
                pname = Some(lexer.parse_string()?);
            } else {
                lexer.skip_string()?;
            }
            lexer.expect(b')')?;

            if lexer.peek() == Some(b',') {
                lexer.next_char();
            } else {
                break;
            }
        }
    }
    lexer.expect(b']')?;
    lexer.expect(b')')?;

    Ok(ParsedDerivation {
        outputs,
        input_drvs,
        input_srcs,
        platform,
        builder: String::new(),
        args: Vec::new(),
        env: HashMap::new(),
        pname,
    })
}
