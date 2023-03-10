use crate::{error::ParserError, macros::*, Parser, ParserResult};

use ry_ast::*;
use ry_ast::{
    location::{Span, WithSpan},
    token::*,
};

impl<'c> Parser<'c> {
    pub(crate) fn parse_name(&mut self) -> ParserResult<WithSpan<String>> {
        let start = self.current.span.range.start;

        let mut name = self.current.value.ident().unwrap();
        name.push_str("::");

        let mut end = self.current.span.range.end;

        self.advance()?; // id

        while self.current.value.is(&RawToken::DoubleColon) {
            self.advance()?; // '::'

            check_token0!(self, "identifier", RawToken::Identifier(_), "name")?;

            name.push_str(&self.current.value.ident().unwrap());
            name.push_str("::");

            end = self.current.span.range.end;

            self.advance()?; // id
        }

        name.pop();
        name.pop();

        Ok((name, (start..end).into()).into())
    }

    pub(crate) fn parse_type(&mut self) -> ParserResult<Type> {
        let start = self.current.span.range.start;

        let mut lhs = match &self.current.value {
            RawToken::Identifier(_) => self.parse_primary_type(),
            RawToken::Asterisk => self.parse_pointer_type(),
            RawToken::OpenBracket => self.parse_array_type(),
            _ => Err(ParserError::UnexpectedToken(
                self.current.clone(),
                "type".into(),
                None,
            )),
        }?;

        while self.current.value.is(&RawToken::QuestionMark) {
            lhs = WithSpan::new(
                Box::new(RawType::Option(lhs)),
                Span::new(start, self.current.span.range.end),
            );
            self.advance()?;
        }

        Ok(lhs)
    }

    fn parse_primary_type(&mut self) -> ParserResult<Type> {
        let start = self.current.span.range.end;
        let name = self.parse_name()?;
        let mut end = self.current.span.range.end;
        let generic_part = self.parse_type_generic_part()?;

        if generic_part.is_some() {
            end = self.previous.as_ref().unwrap().span.range.end;
        }

        Ok(WithSpan::new(
            Box::new(RawType::Primary(
                name,
                if let Some(v) = generic_part {
                    v
                } else {
                    vec![]
                },
            )),
            Span::new(start, end),
        ))
    }

    pub(crate) fn parse_type_generic_part(&mut self) -> ParserResult<Option<Vec<Type>>> {
        if self.current.value.is(&RawToken::LessThan) {
            self.advance()?; // '<'

            Ok(Some(parse_list!(
                self,
                "generics",
                &RawToken::GreaterThan,
                false,
                || self.parse_type()
            )))
        } else {
            Ok(None)
        }
    }

    fn parse_array_type(&mut self) -> ParserResult<Type> {
        let start = self.current.span.range.start;

        self.advance()?; // '['

        let inner_type = self.parse_type()?;

        check_token!(self, RawToken::CloseBracket, "array type")?;

        let end = self.current.span.range.end;

        self.advance()?; // ']'

        Ok(WithSpan::new(
            Box::new(RawType::Array(inner_type)),
            Span::new(start, end),
        ))
    }

    fn parse_pointer_type(&mut self) -> ParserResult<Type> {
        let start = self.current.span.range.start;

        self.advance()?; // '*'

        let inner_type = self.parse_type()?;

        let end = self.current.span.range.end;

        Ok(WithSpan::new(
            Box::new(RawType::Pointer(inner_type)),
            Span::new(start, end),
        ))
    }

    pub(crate) fn parse_generic_annotations(&mut self) -> ParserResult<GenericAnnotations> {
        let mut generics = vec![];

        if !self.current.value.is(&RawToken::LessThan) {
            return Ok(generics);
        }

        self.advance()?; // '<'

        if self.current.value.is(&RawToken::GreaterThan) {
            self.advance()?; // '>'
            return Ok(generics);
        }

        loop {
            check_token0!(
                self,
                "identifier",
                RawToken::Identifier(_),
                "generic annotation"
            )?;

            let generic = self.parse_generic()?;

            let mut constraint = None;

            if !self.current.value.is(&RawToken::Comma)
                && !self.current.value.is(&RawToken::GreaterThan)
            {
                constraint = Some(self.parse_type()?);
            }

            generics.push((generic, constraint));

            if !self.current.value.is(&RawToken::Comma) {
                check_token!(self, RawToken::GreaterThan, "generic annotations")?;

                self.advance()?; // >

                return Ok(generics);
            }

            self.advance()?;
        }
    }

    pub fn parse_generic(&mut self) -> ParserResult<WithSpan<String>> {
        let start = self.current.span.range.start;

        let name = self.current.value.ident().unwrap();
        let end = self.current.span.range.end;

        self.advance()?; // id

        Ok(WithSpan::new(name, Span::new(start, end)))
    }
}
