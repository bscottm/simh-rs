// SPDX-License-Identifier: MIT

use nom::{
    branch::alt,
    bytes::complete::tag,
    character::complete::{char, space0},
    combinator::map,
    error::context,
    sequence::preceded,
    Parser,
};

use crate::cli::{
    cmd_table::CLIResult, parsers::numerics::parse_scalar, parsers::types::InputRadix, span::Span,
};

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// Masking and search expressions
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// Mask operation: "&", "|" and "^" `<value>`
///
/// Value masking operations for the EXAMINE and DEPOSIT commands.
#[derive(Debug, Clone, PartialEq)]
pub enum MaskOperation {
    And(u64),
    Or(u64),
    Xor(u64),
}

/// Search/comparison operation
#[derive(Debug, Clone, PartialEq)]
pub enum SearchOperation {
    Equal(u64),
    NotEqual(u64),
    GreaterEqual(u64),
    Greater(u64),
    LessEqual(u64),
    Less(u64),
}

impl MaskOperation {
    /// Apply the mask operation to a value
    pub fn apply(&self, val: u64) -> u64 {
        match self {
            Self::And(mask) => val & mask,
            Self::Or(mask) => val | mask,
            Self::Xor(mask) => val ^ mask,
        }
    }
}

impl SearchOperation {
    /// Test if a value matches the search criteria
    pub fn matches(&self, val: u64) -> bool {
        match self {
            Self::Equal(target) => val == *target,
            Self::NotEqual(target) => val != *target,
            Self::GreaterEqual(target) => val >= *target,
            Self::Greater(target) => val > *target,
            Self::LessEqual(target) => val <= *target,
            Self::Less(target) => val < *target,
        }
    }
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// Masking, search expression implementation
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=

/// Parse a mask operation: `[&|^] value`
pub fn parse_mask_op<'a>(radix: InputRadix) -> impl FnMut(Span<'a>) -> CLIResult<'a, MaskOperation> {
    move |span: Span<'a>| {
        let (span, _) = space0.parse(span)?;

        context(
            "mask operation",
            alt((
                map(
                    preceded(char('&'), preceded(space0, parse_scalar(radix))),
                    MaskOperation::And,
                ),
                map(
                    preceded(char('|'), preceded(space0, parse_scalar(radix))),
                    MaskOperation::Or,
                ),
                map(
                    preceded(char('^'), preceded(space0, parse_scalar(radix))),
                    MaskOperation::Xor,
                ),
            )),
        )
        .parse(span)
    }
}

/// Parse a search operation: `[==|!=|>=|>|<=|<] value`
pub fn parse_search_op<'a>(radix: InputRadix) -> impl FnMut(Span<'a>) -> CLIResult<'a, SearchOperation> {
    move |span: Span<'a>| {
        let (span, _) = space0.parse(span)?;

        context(
            "search operation",
            alt((
                // Order matters! Parse == before =, >= before >, etc.
                map(
                    preceded(tag("=="), preceded(space0, parse_scalar(radix))),
                    SearchOperation::Equal,
                ),
                map(
                    preceded(tag("!="), preceded(space0, parse_scalar(radix))),
                    SearchOperation::NotEqual,
                ),
                map(
                    preceded(tag(">="), preceded(space0, parse_scalar(radix))),
                    SearchOperation::GreaterEqual,
                ),
                map(
                    preceded(tag("<="), preceded(space0, parse_scalar(radix))),
                    SearchOperation::LessEqual,
                ),
                map(
                    preceded(char('>'), preceded(space0, parse_scalar(radix))),
                    SearchOperation::Greater,
                ),
                map(
                    preceded(char('<'), preceded(space0, parse_scalar(radix))),
                    SearchOperation::Less,
                ),
            )),
        )
        .parse(span)
    }
}

#[cfg(test)]
mod mask_search_tests {
    use super::*;

    #[test]
    fn test_mask_operations() {
        let (_, op) = parse_mask_op(InputRadix::Hex)(Span::new("& 0xFF")).unwrap();
        assert_eq!(op, MaskOperation::And(0xFF));

        let (_, op) = parse_mask_op(InputRadix::Hex)(Span::new("&0xfF")).unwrap();
        assert_eq!(op, MaskOperation::And(0xFF));

        let (_, op) = parse_mask_op(InputRadix::Hex)(Span::new("&fC")).unwrap();
        assert_eq!(op, MaskOperation::And(0xFC));

        let (_, op) = parse_mask_op(InputRadix::Oct)(Span::new("| 0o77")).unwrap();
        assert_eq!(op, MaskOperation::Or(0o77));

        let (_, op) = parse_mask_op(InputRadix::Oct)(Span::new("|77")).unwrap();
        assert_eq!(op, MaskOperation::Or(0o77));

        let (_, op) = parse_mask_op(InputRadix::Oct)(Span::new("| 71")).unwrap();
        assert_eq!(op, MaskOperation::Or(0o71));

        let (_, op) = parse_mask_op(InputRadix::Oct)(Span::new("|0xea")).unwrap();
        assert_eq!(op, MaskOperation::Or(0xea));

        let (_, op) = parse_mask_op(InputRadix::Dec)(Span::new("^ 255")).unwrap();
        assert_eq!(op, MaskOperation::Xor(255));
    }

    #[test]
    fn test_search_operations() {
        let (_, op) = parse_search_op(InputRadix::Dec)(Span::new("== 42")).unwrap();
        assert_eq!(op, SearchOperation::Equal(42));

        let (_, op) = parse_search_op(InputRadix::Hex)(Span::new(">= 0x100")).unwrap();
        assert_eq!(op, SearchOperation::GreaterEqual(0x100));

        let (_, op) = parse_search_op(InputRadix::Dec)(Span::new("!= 0")).unwrap();
        assert_eq!(op, SearchOperation::NotEqual(0));
    }

    #[test]
    fn test_mask_apply() {
        assert_eq!(MaskOperation::And(0x0F).apply(0xFF), 0x0F);
        assert_eq!(MaskOperation::Or(0x0F).apply(0xF0), 0xFF);
        assert_eq!(MaskOperation::Xor(0xFF).apply(0xAA), 0x55);
    }

    #[test]
    fn test_search_matches() {
        assert!(SearchOperation::Equal(42).matches(42));
        assert!(!SearchOperation::Equal(42).matches(43));
        assert!(SearchOperation::Greater(10).matches(11));
        assert!(!SearchOperation::Greater(10).matches(10));
    }
}
