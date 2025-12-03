// SPDX-License-Identifier: MIT

use nom::{
    error::{ErrorKind, ParseError},
    IResult,
};

/// Spans track input location
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span<'a> {
    pub input: &'a str,
    pub offset: usize, // Byte offset from start of original
    pub line: usize,   // Line number (1-indexed)
    pub column: usize, // Column within line (1-indexed)
}

//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
// Span implementation:
//=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=~=
impl<'a> Span<'a> {
    pub fn new(input: &'a str) -> Self {
        Self::new_with_line(input, 1)
    }

    pub fn new_with_line(input: &'a str, line: usize) -> Self {
        Span {
            input,
            offset: 0,
            line,
            column: 1,
        }
    }

    // Helper to advance position tracking
    fn advance(&self, consumed: usize) -> Self {
        let fragment = &self.input[..consumed];
        let newlines = fragment.chars().filter(|&c| c == '\n').count();

        let column = if newlines > 0 {
            // Reset to 1 after last newline
            fragment.chars().rev().take_while(|&c| c != '\n').count() + 1
        } else {
            self.column + fragment.chars().count()
        };

        Span {
            input: &self.input[consumed..],
            offset: self.offset + consumed,
            line: self.line + newlines,
            column,
        }
    }
}

impl<'a> Default for Span<'a> {
    fn default() -> Self {
        Self {
            input: Default::default(),
            offset: Default::default(),
            line: Default::default(),
            column: Default::default(),
        }
    }
}

// The main Input trait for nom 8
impl<'a> nom::Input for Span<'a> {
    type Item = char;
    type Iter = std::str::Chars<'a>;
    type IterIndices = std::str::CharIndices<'a>;

    fn input_len(&self) -> usize {
        self.input.len()
    }

    fn take(&self, count: usize) -> Self {
        Span {
            input: &self.input[..count],
            ..*self
        }
    }

    fn take_from(&self, count: usize) -> Self {
        self.advance(count)
    }

    fn take_split(&self, count: usize) -> (Self, Self) {
        (self.advance(count), self.take(count))
    }

    fn position<P>(&self, predicate: P) -> Option<usize>
    where
        P: Fn(Self::Item) -> bool,
    {
        self.input.chars().position(predicate)
    }

    fn iter_elements(&self) -> Self::Iter {
        self.input.chars()
    }

    fn iter_indices(&self) -> Self::IterIndices {
        self.input.char_indices()
    }

    fn slice_index(&self, count: usize) -> Result<usize, nom::Needed> {
        if self.input.len() >= count {
            Ok(count)
        } else {
            Err(nom::Needed::new(count - self.input.len()))
        }
    }

    fn split_at_position<P, E>(&self, predicate: P) -> IResult<Self, Self, E>
    where
        P: Fn(Self::Item) -> bool,
        E: ParseError<Self>,
    {
        match self.input.chars().position(predicate) {
            Some(n) => Ok(self.take_split(n)),
            None => Err(nom::Err::Incomplete(nom::Needed::new(1))),
        }
    }

    fn split_at_position1<P, E>(&self, predicate: P, e: ErrorKind) -> IResult<Self, Self, E>
    where
        P: Fn(Self::Item) -> bool,
        E: ParseError<Self>,
    {
        match self.input.chars().position(predicate) {
            Some(0) => Err(nom::Err::Error(E::from_error_kind(*self, e))),
            Some(n) => Ok(self.take_split(n)),
            None => Err(nom::Err::Incomplete(nom::Needed::new(1))),
        }
    }

    fn split_at_position_complete<P, E>(&self, predicate: P) -> IResult<Self, Self, E>
    where
        P: Fn(Self::Item) -> bool,
        E: ParseError<Self>,
    {
        match self.input.chars().position(predicate) {
            Some(n) => Ok(self.take_split(n)),
            None => Ok(self.take_split(self.input.len())),
        }
    }

    fn split_at_position1_complete<P, E>(&self, predicate: P, e: ErrorKind) -> IResult<Self, Self, E>
    where
        P: Fn(Self::Item) -> bool,
        E: ParseError<Self>,
    {
        match self.input.chars().position(predicate) {
            Some(0) => Err(nom::Err::Error(E::from_error_kind(*self, e))),
            Some(n) => Ok(self.take_split(n)),
            None => {
                if self.input.is_empty() {
                    Err(nom::Err::Error(E::from_error_kind(*self, e)))
                } else {
                    Ok(self.take_split(self.input.len()))
                }
            }
        }
    }
}

// Additional traits that may still be needed
impl<'a> nom::Compare<&str> for Span<'a> {
    fn compare(&self, t: &str) -> nom::CompareResult {
        self.input.compare(t)
    }

    fn compare_no_case(&self, t: &str) -> nom::CompareResult {
        self.input.compare_no_case(t)
    }
}

impl<'a> nom::FindSubstring<&str> for Span<'a> {
    fn find_substring(&self, substr: &str) -> Option<usize> {
        self.input.find(substr)
    }
}

impl<'a> nom::Offset for Span<'a> {
    fn offset(&self, second: &Self) -> usize {
        second.offset - self.offset
    }
}

impl<'a, T> nom::ParseTo<T> for Span<'a>
where
    &'a str: nom::ParseTo<T>,
{
    fn parse_to(&self) -> Option<T> {
        self.input.parse_to()
    }
}
