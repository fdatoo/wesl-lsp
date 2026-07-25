#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

/// How a client counts columns. UTF-16 is the protocol default and every client supports it;
/// UTF-8 is worth negotiating because this crate is byte-offset native, so it removes the
/// conversion entirely rather than replacing it with a different one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PositionEncoding {
    #[default]
    Utf16,
    Utf8,
}

#[derive(Clone, Debug)]
pub struct LineIndex {
    line_starts: Vec<usize>,
    encoding: PositionEncoding,
}

impl LineIndex {
    pub fn new(source: &str, encoding: PositionEncoding) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(
            source
                .bytes()
                .enumerate()
                .filter_map(|(offset, byte)| (byte == b'\n').then_some(offset + 1)),
        );
        Self {
            line_starts,
            encoding,
        }
    }

    pub fn offset_to_position(&self, source: &str, offset: usize) -> Option<Position> {
        if offset > source.len() || !source.is_char_boundary(offset) {
            return None;
        }
        let line = self.line_starts.partition_point(|&start| start <= offset) - 1;
        let line_start = self.line_starts[line];
        let character = match self.encoding {
            PositionEncoding::Utf8 => offset - line_start,
            PositionEncoding::Utf16 => source[line_start..offset].encode_utf16().count(),
        };
        Some(Position {
            line: line.try_into().ok()?,
            character: character.try_into().ok()?,
        })
    }

    pub fn position_to_offset(&self, source: &str, position: Position) -> Option<usize> {
        let line_start = *self.line_starts.get(position.line as usize)?;
        let line_end = self
            .line_starts
            .get(position.line as usize + 1)
            .copied()
            .map_or(source.len(), |next| next.saturating_sub(1));
        let target = position.character as usize;

        if self.encoding == PositionEncoding::Utf8 {
            let offset = line_start + target;
            // Still validated: a column past the line, or mid-character, is not addressable.
            return (offset <= line_end && source.is_char_boundary(offset)).then_some(offset);
        }

        let line = &source[line_start..line_end];
        let mut utf16_col = 0;
        for (byte_offset, ch) in line.char_indices() {
            if utf16_col == target {
                return Some(line_start + byte_offset);
            }
            utf16_col += ch.len_utf16();
            if utf16_col > target {
                return None;
            }
        }
        (utf16_col == target).then_some(line_end)
    }
}

#[cfg(test)]
mod tests {
    use super::{LineIndex, Position, PositionEncoding};

    #[test]
    fn positions_use_utf8_columns_when_negotiated() {
        let source = "a😀b\néx";
        let index = LineIndex::new(source, PositionEncoding::Utf8);

        // Under UTF-8 the column is the byte offset within the line, so the emoji counts 4.
        let b = source.find('b').unwrap();
        assert_eq!(
            index.offset_to_position(source, b),
            Some(Position {
                line: 0,
                character: 5,
            })
        );
        assert_eq!(
            index.position_to_offset(
                source,
                Position {
                    line: 0,
                    character: 5,
                },
            ),
            Some(b)
        );

        // A column landing inside the emoji is still rejected rather than silently rounded.
        assert_eq!(
            index.position_to_offset(
                source,
                Position {
                    line: 0,
                    character: 3,
                },
            ),
            None
        );

        // Round-trips for every boundary on the second line.
        let line_start = source.find('é').unwrap();
        for offset in [line_start, line_start + 2, source.len()] {
            let position = index.offset_to_position(source, offset).unwrap();
            assert_eq!(index.position_to_offset(source, position), Some(offset));
        }
    }

    #[test]
    fn positions_use_utf16_columns() {
        let source = "a😀b\néx";
        let index = LineIndex::new(source, PositionEncoding::Utf16);
        let b = source.find('b').unwrap();
        assert_eq!(
            index.offset_to_position(source, b),
            Some(Position {
                line: 0,
                character: 3,
            })
        );
        assert_eq!(
            index.position_to_offset(
                source,
                Position {
                    line: 0,
                    character: 3,
                },
            ),
            Some(b)
        );
        assert_eq!(
            index.position_to_offset(
                source,
                Position {
                    line: 0,
                    character: 2,
                },
            ),
            None
        );
    }
}
