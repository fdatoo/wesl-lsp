#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

#[derive(Clone, Debug)]
pub struct LineIndex {
    line_starts: Vec<usize>,
}

impl LineIndex {
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(
            source
                .bytes()
                .enumerate()
                .filter_map(|(offset, byte)| (byte == b'\n').then_some(offset + 1)),
        );
        Self { line_starts }
    }

    pub fn offset_to_position(&self, source: &str, offset: usize) -> Option<Position> {
        if offset > source.len() || !source.is_char_boundary(offset) {
            return None;
        }
        let line = self.line_starts.partition_point(|&start| start <= offset) - 1;
        let line_start = self.line_starts[line];
        let character = source[line_start..offset].encode_utf16().count();
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
        let line = &source[line_start..line_end];
        let target = position.character as usize;
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
    use super::{LineIndex, Position};

    #[test]
    fn positions_use_utf16_columns() {
        let source = "a😀b\néx";
        let index = LineIndex::new(source);
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
