//! lexer/mod.rs - implements lexer.
//!
//! Lexer implements `Iterator<Item = Token>`.

use crate::ast::location::*;
use crate::ast::token::*;
use std::char::from_digit;
use std::{path::Path, str::Chars};

pub struct Lexer<'a> {
    current: char,
    next: char,
    filename: &'a Path,
    contents: &'a str,
    chars: Chars<'a>,
    location: Location,
    start_location: Location,
}

fn decimal(c: char) -> bool {
    return '0' <= c && c <= '9';
}

fn hexadecimal(c: char) -> bool {
    return '0' <= c && c <= '9' || 'a' <= c.to_ascii_lowercase() && c.to_ascii_lowercase() <= 'f';
}

fn invalid_seperator(buffer: String) -> i32 {
    let mut x1 = ' ';
    let mut d = '.';
    let mut i = 0;

    let bytes = buffer.as_bytes();

    if buffer.len() >= 2 && bytes[0] as char == '0' {
        x1 = bytes[1] as char;
        if x1 == 'x' || x1 == 'o' || x1 == 'b' {
            d = '0';
            i = 2;
        }
    }

    while i < buffer.len() {
        let p = d;
        d = bytes[i] as char;
        if d == '_' {
            if p != '0' {
                return i as i32;
            }
        } else {
            if decimal(d) || x1 == 'x' && hexadecimal(d) {
                d = '0';
            }

            if p == '_' {
                return i as i32 - 1;
            }

            d = '.';
        }
        i += 1;
    }

    if d == '_' {
        return bytes.len() as i32 - 1;
    }

    -1
}

type IterElem<'a> = Option<Token<'a>>;

impl<'a> Lexer<'a> {
    pub fn new(filename: &'a Path, contents: &'a str) -> Self {
        let mut chars = contents.chars();

        let current = chars.next().unwrap_or('\0');
        let next = chars.next().unwrap_or('\0');

        Self {
            current,
            next,
            filename,
            contents,
            chars,
            location: Location::start(),
            start_location: Location::start(),
        }
    }

    fn eof(&self) -> bool {
        self.current == '\0'
    }

    fn skip_over_whitespaces(&mut self) {
        while self.current.is_whitespace() {
            self.advance();
        }
    }

    fn advance(&mut self) {
        let previous = self.current;

        self.current = self.next;
        self.next = self.chars.next().unwrap_or('\0');

        self.location.advance(previous.len_utf8(), previous == '\n');
    }

    fn advance_twice(&mut self) {
        self.advance();
        self.advance();
    }

    fn char_location(&self, character_len: usize) -> Span<'a> {
        Span {
            filename: self.filename,
            start: self.location,
            end: Location {
                index: self.location.index + character_len,
                line: self.location.line,
                column: self.location.column + 1,
            },
        }
    }

    fn advance_with(&mut self, raw: RawToken) -> IterElem<'a> {
        self.advance();
        Some(Token::new(raw, self.char_location(1)))
    }

    fn advance_while<F>(&mut self, mut f: F) -> &'a str
    where
        F: FnMut(char, char) -> bool,
    {
        while f(self.current, self.next) && !self.eof() {
            self.advance();
        }

        &self.contents[self.start_location.index..self.location.index]
    }

    fn span_from_start(&self) -> Span<'a> {
        Span::new(self.filename, self.start_location, self.location)
    }

    fn scan_string(&mut self) -> IterElem<'a> {
        self.start_location = self.location;

        self.advance(); // '"'

        let content = self.advance_while(|current, _| current != '"' && current != '\n');

        if self.eof() || self.current == '\n' {
            return Some(Token::new(
                RawToken::Invalid(LexerError::UnterminatedStringLiteral),
                self.span_from_start(),
            ));
        }

        self.advance(); // '"'

        Some(Token::new(
            RawToken::String(content[1..].to_owned()),
            self.span_from_start(),
        ))
    }

    fn scan_wrapped_id(&mut self) -> IterElem<'a> {
        self.start_location = self.location;

        self.advance(); // '`'

        let content = self.advance_while(|current, _| current != '`');

        if self.eof() || self.current == '\n' {
            return Some(Token::new(
                RawToken::Invalid(LexerError::UnterminatedWrappedIdentifierLiteral),
                self.span_from_start(),
            ));
        }

        self.advance(); // '`'

        Some(Token::new(
            RawToken::Identifier(content[1..].to_owned()),
            Span::new(self.filename, self.start_location, self.location),
        ))
    }

    fn scan_single_line_comment(&mut self) -> IterElem<'a> {
        self.advance_twice();

        self.start_location = self.location;

        let content = self.advance_while(|current, _| (current != '\n'));

        Some(Token::new(
            RawToken::Comment(content.to_owned()),
            self.span_from_start(),
        ))
    }

    fn scan_name(&mut self) -> IterElem<'a> {
        self.start_location = self.location;
        let name = self.advance_while(|current, _| current.is_alphanumeric() || current == '_');

        match RESERVED.get(name) {
            Some(reserved) => Some(Token::new(reserved.clone(), self.span_from_start())),
            None => Some(Token::new(
                RawToken::Identifier(name.to_owned()),
                self.span_from_start(),
            )),
        }
    }

    fn scan_digits(
        &mut self,
        base: i8,
        invalid_digit_location: &mut Option<Location>,
        digit_seperator: &mut i32,
    ) {
        if base <= 10 {
            let max = from_digit((base - 1) as u32, 10).unwrap();
            // let ds = 1;

            while decimal(self.current) || self.current == '_' {
                let mut ds = 1;

                if self.current == '_' {
                    ds = 2;
                } else if self.current >= max && invalid_digit_location.is_none() {
                    *invalid_digit_location = Some(self.location);
                }

                *digit_seperator |= ds;
                self.advance();
            }
        } else {
            while hexadecimal(self.current) || self.current == '_' {
                let mut ds = 1;

                if self.current == '_' {
                    ds = 2;
                }

                *digit_seperator |= ds;
                self.advance();
            }
        }
    }

    fn scan_number(&mut self) -> IterElem<'a> {
        self.start_location = self.location;

        let mut number_kind = NumberKind::Invalid;

        let mut base: i8 = 10;
        let mut prefix = '0';
        let mut digit_seperator = 0;

        let mut invalid_digit_location: Option<Location> = None;

        if self.current != '.' {
            number_kind = NumberKind::Int;

            if self.current == '0' {
                self.advance();

                match self.current.to_ascii_lowercase() {
                    'x' => {
                        self.advance();
                        base = 16;
                        prefix = 'x';
                    }
                    'o' => {
                        self.advance();
                        base = 8;
                        prefix = 'o';
                    }
                    'b' => {
                        self.advance();
                        base = 2;
                        prefix = 'b';
                    }
                    _ => {
                        base = 8;
                        prefix = '0';
                        digit_seperator = 1;
                    }
                }
            }

            self.scan_digits(base, &mut invalid_digit_location, &mut digit_seperator);
        }

        // fractional part
        if self.current == '.' {
            number_kind = NumberKind::Float;

            if prefix == 'o' || prefix == 'b' || prefix == 'x' {
                return Some(Token::new(
                    RawToken::Invalid(LexerError::InvalidRadixPoint),
                    self.span_from_start(),
                ));
            }

            self.advance();
            self.scan_digits(base, &mut invalid_digit_location, &mut digit_seperator);
        }

        if digit_seperator & 1 == 0 {
            return Some(Token::new(
                RawToken::Invalid(LexerError::HasNoDigits),
                self.span_from_start(),
            ));
        }

        let l = self.current.to_ascii_lowercase();
        if l == 'e' {
            if l == 'e' && prefix != '\0' && prefix != '0' {
                return Some(Token::new(
                    RawToken::Invalid(LexerError::ExponentRequiresDecimalMantissa),
                    self.span_from_start(),
                ));
            }

            self.advance();

            number_kind = NumberKind::Float;

            if self.current == '+' || self.current == '-' {
                self.advance();
            }

            let mut ds = 0;
            self.scan_digits(10, &mut None, &mut ds);
            digit_seperator |= ds;

            if ds & 1 == 0 {
                return Some(Token::new(
                    RawToken::Invalid(LexerError::ExponentHasNoDigits),
                    self.span_from_start(),
                ));
            }
        }

        if self.current == 'i' {
            number_kind = NumberKind::Imag;
            self.advance();
        }

        let buffer = &self.contents[self.start_location.index..self.location.index];

        if number_kind == NumberKind::Int && invalid_digit_location.is_some() {
            return Some(Token::new(
                RawToken::Invalid(LexerError::InvalidDigit),
                invalid_digit_location
                    .unwrap()
                    .char_location(self.filename, 1),
            ));
        }

        if digit_seperator & 2 != 0 {
            if invalid_seperator(buffer.to_owned()) >= 0 {
                return Some(Token::new(
                    RawToken::Invalid(LexerError::UnderscoreMustSeperateSuccessiveDigits),
                    self.span_from_start(),
                ));
            }
        }

        match number_kind {
            NumberKind::Invalid => None,
            NumberKind::Int => {
                return Some(Token::new(
                    RawToken::Int(buffer.parse().unwrap()),
                    self.span_from_start(),
                ));
            }
            NumberKind::Float => {
                return Some(Token::new(
                    RawToken::Float(buffer.parse().unwrap()),
                    self.span_from_start(),
                ));
            }
            NumberKind::Imag => {
                return Some(Token::new(
                    RawToken::Imag(buffer[..buffer.len() - 1].parse().unwrap()),
                    self.span_from_start(),
                ));
            }
        }
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Token<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current.is_whitespace() {
            self.skip_over_whitespaces();
        }

        match (self.current, self.next) {
            ('\0', _) => self.advance_with(RawToken::EndOfFile),

            ('"', _) => self.scan_string(),
            ('`', _) => self.scan_wrapped_id(),

            ('+', '+') => self.advance_with(RawToken::PlusPlus),
            ('+', '=') => self.advance_with(RawToken::PlusEq),
            ('+', _) => self.advance_with(RawToken::Plus),

            ('-', '-') => self.advance_with(RawToken::MinusMinus),
            ('-', '=') => self.advance_with(RawToken::MinusEq),
            ('-', _) => self.advance_with(RawToken::Minus),

            ('*', '=') => self.advance_with(RawToken::AsteriskEq),
            ('*', _) => self.advance_with(RawToken::Asterisk),

            ('/', '/') => self.scan_single_line_comment(),
            ('/', '=') => self.advance_with(RawToken::SlashEq),
            ('/', _) => self.advance_with(RawToken::Slash),

            ('!', '=') => self.advance_with(RawToken::NotEq),
            ('!', _) => self.advance_with(RawToken::Bang),

            ('>', '>') => self.advance_with(RawToken::RightShift),
            ('>', '=') => self.advance_with(RawToken::GreaterThanOrEq),
            ('>', _) => self.advance_with(RawToken::GreaterThan),

            ('<', '<') => self.advance_with(RawToken::LeftShift),
            ('<', '=') => self.advance_with(RawToken::LessThanOrEq),
            ('<', _) => self.advance_with(RawToken::LessThan),

            ('=', '=') => self.advance_with(RawToken::Eq),
            ('=', _) => self.advance_with(RawToken::Assign),

            ('|', '=') => self.advance_with(RawToken::OrEq),
            ('|', '|') => self.advance_with(RawToken::OrOr),
            ('|', _) => self.advance_with(RawToken::Or),

            ('&', '&') => self.advance_with(RawToken::AndAnd),
            ('&', _) => self.advance_with(RawToken::And),

            ('^', '=') => self.advance_with(RawToken::XorEq),
            ('^', _) => self.advance_with(RawToken::Xor),

            ('~', '=') => self.advance_with(RawToken::NotEq),
            ('~', _) => self.advance_with(RawToken::Not),

            ('(', _) => self.advance_with(RawToken::OpenParent),
            (')', _) => self.advance_with(RawToken::CloseParent),

            ('[', _) => self.advance_with(RawToken::OpenBracket),
            (']', _) => self.advance_with(RawToken::CloseBracket),

            ('{', _) => self.advance_with(RawToken::OpenBrace),
            ('}', _) => self.advance_with(RawToken::CloseBrace),

            (',', _) => self.advance_with(RawToken::Comma),
            (';', _) => self.advance_with(RawToken::Semicolon),

            (c, _) => {
                if decimal(self.current) || (self.current == '_' && decimal(self.next)) {
                    return self.scan_number();
                } else if c.is_alphanumeric() || c == '_' {
                    return self.scan_name();
                } else if c == '.' {
                    return self.advance_with(RawToken::Dot);
                }

                self.advance_with(RawToken::Invalid(LexerError::UnexpectedChar(c)))
            }
        }
    }
}

#[cfg(test)]
mod lexer_tests {
    use crate::ast::token::*;
    use crate::lexer::Lexer;
    use std::path::Path;

    macro_rules! def_lex {
        ($l: ident, $contents: expr) => {
            let mut $l = Lexer::new(Path::new("<test>"), $contents);
        };
    }

    #[test]
    fn eof_test() {
        def_lex!(l, "");
        assert_eq!(l.next().unwrap().raw, RawToken::EndOfFile);
    }

    #[test]
    fn eof2_test() {
        def_lex!(l, " \t\n\r");
        assert_eq!(l.next().unwrap().raw, RawToken::EndOfFile);
    }

    #[test]
    fn identifier_test() {
        def_lex!(l, "test");
        assert_eq!(
            l.next().unwrap().raw,
            RawToken::Identifier("test".to_owned())
        );
    }

    #[test]
    fn identifier2_test() {
        def_lex!(l, "привет");
        assert_eq!(
            l.next().unwrap().raw,
            RawToken::Identifier("привет".to_owned())
        );
    }

    #[test]
    fn comment_test() {
        def_lex!(l, "//test comment");
        assert_eq!(
            l.next().unwrap().raw,
            RawToken::Comment("test comment".to_owned())
        );
    }

    #[test]
    fn unexpected_char_test() {
        def_lex!(l, "@");
        assert_eq!(
            l.next().unwrap().raw,
            RawToken::Invalid(LexerError::UnexpectedChar('@'))
        );
    }

    #[test]
    fn string_test() {
        def_lex!(l, "\"test\"");
        assert_eq!(l.next().unwrap().raw, RawToken::String("test".to_owned()));
    }

    #[test]
    fn string2_test() {
        def_lex!(l, "\"test");
        assert_eq!(
            l.next().unwrap().raw,
            RawToken::Invalid(LexerError::UnterminatedStringLiteral)
        );
    }

    #[test]
    fn string3_test() {
        def_lex!(l, "\"test\n");
        assert_eq!(
            l.next().unwrap().raw,
            RawToken::Invalid(LexerError::UnterminatedStringLiteral)
        );
    }

    #[test]
    fn wrapped_id_test() {
        def_lex!(l, "`test`");
        assert_eq!(
            l.next().unwrap().raw,
            RawToken::Identifier("test".to_owned())
        );
    }

    #[test]
    fn wrapped_id2_test() {
        def_lex!(l, "`test");
        assert_eq!(
            l.next().unwrap().raw,
            RawToken::Invalid(LexerError::UnterminatedWrappedIdentifierLiteral)
        );
    }

    #[test]
    fn wrapped_id3_test() {
        def_lex!(l, "`test\n");
        assert_eq!(
            l.next().unwrap().raw,
            RawToken::Invalid(LexerError::UnterminatedWrappedIdentifierLiteral)
        );
    }

    #[test]
    fn number_test() {
        def_lex!(l, "12345");
        assert_eq!(l.next().unwrap().raw, RawToken::Int(12345));
    }

    #[test]
    fn number2_test() {
        def_lex!(l, "12345.12345");
        assert_eq!(l.next().unwrap().raw, RawToken::Float(12345.12345));
    }

    #[test]
    fn number3_test() {
        def_lex!(l, "12345.");
        assert_eq!(l.next().unwrap().raw, RawToken::Float(12345f64));
    }

    #[test]
    fn number4_test() {
        def_lex!(l, "1e3");
        assert_eq!(l.next().unwrap().raw, RawToken::Float(1000f64));
    }

    #[test]
    fn number5_test() {
        def_lex!(l, "0b");
        assert_eq!(
            l.next().unwrap().raw,
            RawToken::Invalid(LexerError::HasNoDigits)
        );
    }

    #[test]
    fn number6_test() {
        def_lex!(l, "12.3e");
        assert_eq!(
            l.next().unwrap().raw,
            RawToken::Invalid(LexerError::ExponentHasNoDigits)
        );
    }

    #[test]
    fn number7_test() {
        def_lex!(l, "0x0.");
        assert_eq!(
            l.next().unwrap().raw,
            RawToken::Invalid(LexerError::InvalidRadixPoint)
        );
    }

    #[test]
    fn number8_test() {
        def_lex!(l, "0b_0");
        assert_eq!(
            l.next().unwrap().raw,
            RawToken::Invalid(LexerError::UnderscoreMustSeperateSuccessiveDigits)
        );
    }
}
